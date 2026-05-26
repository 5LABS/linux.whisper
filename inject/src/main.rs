//! whisper-inject – Textinjektion über das XDG-RemoteDesktop-Portal via EIS/libei.
//!
//! Zwei Rollen in einer Binary:
//!   `whisper-inject daemon`  hält eine RemoteDesktop-Session offen, baut eine
//!                            EIS-Verbindung auf und lauscht auf einem Unix-Socket;
//!                            eingehender Text wird per EIS-Keyboard ins fokussierte
//!                            Fenster getippt.
//!   `whisper-inject`         liest Text von stdin und schickt ihn an den Daemon.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use ashpd::desktop::{
    PersistMode,
    remote_desktop::{ConnectToEISOptions, DeviceType, RemoteDesktop, SelectDevicesOptions},
};
use enumflags2::BitFlags;
use futures::StreamExt;
use reis::{
    ei,
    event::{DeviceCapability, EiEvent},
    tokio::EiConvertEventStream,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use xkbcommon::xkb;

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("daemon") => run_daemon().await,
        _ => run_client().await,
    }
}

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.join("whisper-inject.sock")
}

fn token_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").expect("HOME nicht gesetzt");
            PathBuf::from(home).join(".local/state")
        });
    base.join("whisper-dictate").join("restore_token")
}

fn load_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn save_token(token: &str) -> Result<()> {
    let path = token_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, token)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// EIS-Worker (eigener Thread mit eigenem current-thread tokio-Runtime)
// ---------------------------------------------------------------------------

/// Keymap-Zustand für Zeichensuche.
struct KeymapState {
    keymap: xkb::Keymap,
    shift_keycode: u32,
}

impl KeymapState {
    fn new(keymap: xkb::Keymap) -> Self {
        let shift_keycode = (keymap.min_keycode().raw()..=keymap.max_keycode().raw())
            .find(|&i| {
                keymap
                    .key_get_syms_by_level(xkb::Keycode::new(i), 0, 0)
                    .contains(&xkb::Keysym::Shift_L)
            })
            .unwrap_or(50); // Fallback LSHIFT auf normalen QWERTZ-Tastaturen
        Self { keymap, shift_keycode }
    }

    /// Gibt (keycode_evdev, shift_nötig) für ein Zeichen zurück, oder None.
    fn lookup(&self, ch: char) -> Option<(u32, bool)> {
        let keysym = xkb::Keysym::from_char(ch);
        let all = self.keymap.min_keycode().raw()..=self.keymap.max_keycode().raw();
        for i in all {
            for level in 0u32..=1 {
                let syms = self.keymap.key_get_syms_by_level(
                    xkb::Keycode::new(i),
                    0, // layout/group 0
                    level,
                );
                if syms.contains(&keysym) {
                    // EIS erwartet Linux-Evdev-Keycodes (XKB keycode - 8)
                    return Some((i - 8, level == 1));
                }
            }
        }
        None
    }
}

/// Tipp-Anfrage, die der tokio-Teil an den EIS-Worker sendet.
enum Inject {
    Text(String),
    Shutdown,
}

/// EIS-Worker-Einstiegspunkt: läuft in eigenem Blocking-Thread mit frischem
/// current-thread tokio-Runtime (vermeidet Send-Anforderung für !Send xkb-Typen).
fn eis_worker(eis_stream: StdUnixStream, rx: UnboundedReceiver<Inject>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .context("Tokio-Runtime konnte nicht erstellt werden")?;
    rt.block_on(eis_worker_async(eis_stream, rx))
}

/// Asynchroner EIS-Worker: handhabt Handshake, Setup und Text-Injektion.
/// Läuft ausschließlich auf einem Thread (current_thread-Runtime), daher
/// können !Send-Typen wie xkb::Keymap problemlos verwendet werden.
async fn eis_worker_async(
    eis_stream: StdUnixStream,
    mut rx: UnboundedReceiver<Inject>,
) -> Result<()> {
    let context = ei::Context::new(eis_stream).context("EIS-Context erstellen")?;

    // handshake_tokio liest über EiEventStream, der pending_event() VOR
    // poll_read_ready() prüft – verhindert den Deadlock der blockierenden Version.
    let (conn, mut events): (_, EiConvertEventStream) = context
        .handshake_tokio("whisper-inject", ei::handshake::ContextType::Sender)
        .await
        .context("EIS-Handshake fehlgeschlagen")?;

    let mut keymap_state: Option<KeymapState> = None;
    let mut keyboard_iface: Option<ei::Keyboard> = None;
    let mut keyboard_device: Option<ei::Device> = None;
    let mut sequence: u32 = 0;
    let mut ready = false;

    // Kombinierte Schleife: EIS-Events (Setup + Pings) und Inject-Anfragen
    // werden gleichzeitig behandelt.
    loop {
        tokio::select! {
            biased; // EIS-Events bevorzugen (Pings müssen prompt beantwortet werden)

            maybe_event = events.next() => {
                match maybe_event {
                    None => bail!("EIS-Verbindung unerwartet geschlossen"),
                    Some(Err(e)) => return Err(e.into()),
                    Some(Ok(event)) => {
                        match event {
                            EiEvent::SeatAdded(evt) => {
                                evt.seat.bind_capabilities(DeviceCapability::Keyboard.into());
                                context.flush().context("EIS flush nach bind")?;
                            }
                            EiEvent::DeviceAdded(evt) => {
                                if evt.device.has_capability(DeviceCapability::Keyboard) {
                                    if let Some(km) = evt.device.keymap() {
                                        let xkb_ctx = xkb::Context::new(0);
                                        let keymap = unsafe {
                                            xkb::Keymap::new_from_fd(
                                                &xkb_ctx,
                                                km.fd.try_clone().context("Keymap-FD klonen")?,
                                                km.size as usize,
                                                xkb::KEYMAP_FORMAT_TEXT_V1,
                                                0,
                                            )
                                        }
                                        .context("xkb::Keymap::new_from_fd fehlgeschlagen")?
                                        .context("Keymap war None")?;
                                        keymap_state = Some(KeymapState::new(keymap));
                                    } else {
                                        eprintln!("Warnung: Keyboard-Device ohne Keymap");
                                    }
                                    keyboard_iface = evt.device.interface::<ei::Keyboard>();
                                    keyboard_device = Some(evt.device.device().clone());
                                }
                            }
                            EiEvent::DeviceResumed(_) => {
                                if keyboard_device.is_some() {
                                    ready = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            maybe_inject = rx.recv(), if ready => {
                match maybe_inject {
                    None | Some(Inject::Shutdown) => break,
                    Some(Inject::Text(text)) => {
                        let km = keymap_state.as_ref().context("Keine Keymap")?;
                        let kbd = keyboard_iface.as_ref().context("Kein EIS-Keyboard-Interface")?;
                        let dev = keyboard_device.as_ref().context("Kein EIS-Device")?;

                        dev.start_emulating(sequence, conn.serial());
                        sequence += 1;
                        context.flush()?;

                        for ch in text.chars() {
                            if ch == '\n' || ch == '\r' {
                                kbd.key(28, ei::keyboard::KeyState::Press);
                                kbd.key(28, ei::keyboard::KeyState::Released);
                                context.flush()?;
                                continue;
                            }
                            if ch == '\t' {
                                kbd.key(15, ei::keyboard::KeyState::Press);
                                kbd.key(15, ei::keyboard::KeyState::Released);
                                context.flush()?;
                                continue;
                            }
                            let Some((keycode, shift)) = km.lookup(ch) else {
                                eprintln!("Warnung: kein Keycode für '{ch}' gefunden, überspringe");
                                continue;
                            };
                            if shift {
                                kbd.key(km.shift_keycode - 8, ei::keyboard::KeyState::Press);
                            }
                            kbd.key(keycode, ei::keyboard::KeyState::Press);
                            kbd.key(keycode, ei::keyboard::KeyState::Released);
                            if shift {
                                kbd.key(km.shift_keycode - 8, ei::keyboard::KeyState::Released);
                            }
                            context.flush()?;
                        }

                        dev.frame(conn.serial(), timestamp_us());
                        dev.stop_emulating(conn.serial());
                        context.flush()?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Aktuelle Zeit in Mikrosekunden (CLOCK_MONOTONIC).
fn timestamp_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Client-Modus
// ---------------------------------------------------------------------------

async fn run_client() -> Result<()> {
    let mut buf = Vec::new();
    tokio::io::stdin().read_to_end(&mut buf).await?;
    let text = String::from_utf8_lossy(&buf);
    let text = text.trim_end_matches('\n');
    if text.is_empty() {
        return Ok(());
    }

    let sock = socket_path();
    let mut stream = UnixStream::connect(&sock).await.with_context(|| {
        format!(
            "Portal-Daemon nicht erreichbar ({}). Ist whisper-inject.service aktiv?\n  systemctl --user start whisper-inject.service",
            sock.display()
        )
    })?;
    stream.write_all(text.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon-Modus
// ---------------------------------------------------------------------------

async fn run_daemon() -> Result<()> {
    // --- Portal-Session aufbauen ---
    let proxy = RemoteDesktop::new()
        .await
        .context("RemoteDesktop-Portal nicht verfügbar")?;
    let session = proxy
        .create_session(Default::default())
        .await
        .context("create_session fehlgeschlagen")?;

    let mut opts = SelectDevicesOptions::default()
        .set_devices(BitFlags::from(DeviceType::Keyboard))
        .set_persist_mode(PersistMode::ExplicitlyRevoked);
    if let Some(token) = load_token() {
        opts = opts.set_restore_token(token.as_str());
    }
    proxy
        .select_devices(&session, opts)
        .await?
        .response()
        .context("select_devices fehlgeschlagen")?;

    let started = proxy
        .start(&session, None, Default::default())
        .await?
        .response()
        .context("Start der RemoteDesktop-Session fehlgeschlagen (Dialog abgelehnt?)")?;

    if let Some(token) = started.restore_token() {
        if let Err(e) = save_token(token) {
            eprintln!("Warnung: restore_token konnte nicht gespeichert werden: {e}");
        }
    }
    eprintln!("RemoteDesktop-Session aktiv, Geräte: {:?}", started.devices());

    // --- EIS-Verbindung aufbauen ---
    let eis_fd = proxy
        .connect_to_eis(&session, ConnectToEISOptions::default())
        .await
        .context("connect_to_eis fehlgeschlagen")?;
    let eis_stream = StdUnixStream::from(eis_fd);

    // --- Kanal für Text-Inject-Anfragen (tokio mpsc, Sender ist Send) ---
    let (tx, rx) = mpsc::unbounded_channel::<Inject>();

    // EIS-Worker in eigenem Thread mit frischem current-thread Runtime
    let worker_handle = std::thread::spawn(move || {
        if let Err(e) = eis_worker(eis_stream, rx) {
            eprintln!("EIS-Worker Fehler: {e}");
        }
    });

    // --- Unix-Socket für eingehende Text-Anfragen ---
    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock)
        .with_context(|| format!("Socket {} konnte nicht angelegt werden", sock.display()))?;
    std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
    eprintln!("Lausche auf {}", sock.display());

    loop {
        let (mut conn, _) = listener.accept().await?;
        let mut text = String::new();
        if conn.read_to_string(&mut text).await.is_err() {
            continue;
        }
        let text = text.trim_end_matches('\n').to_string();
        if text.is_empty() {
            continue;
        }
        if tx.send(Inject::Text(text)).is_err() {
            eprintln!("EIS-Worker nicht mehr erreichbar, beende Daemon.");
            break;
        }
    }

    let _ = tx.send(Inject::Shutdown);
    let _ = worker_handle.join();
    Ok(())
}
