# Whisper Diktat – Spracherkennung per Tastendruck

Drücke **Super+Space**, sprich, drücke nochmal **Super+Space** – der erkannte Text wird direkt ins aktive Fenster getippt.

Funktioniert auf Ubuntu 26.04 GNOME (Wayland). Läuft vollständig lokal, kein Internet nötig.

---

## Voraussetzungen

- Ubuntu 26.04 LTS (GNOME, Wayland)
- Mikrofon vorhanden
- ~600 MB freier Speicherplatz (für das Sprachmodell)
- Internetverbindung für die Installation

---

## Installation

### 1. Systempakete installieren

```bash
sudo apt install -y git cmake gcc rustc cargo libxkbcommon-dev alsa-utils
```

### 2. whisper.cpp klonen und kompilieren

```bash
cd /srv/projects/linux.whisper
git clone https://github.com/ggerganov/whisper.cpp
cmake -S whisper.cpp -B whisper.cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build whisper.cpp/build -j$(nproc)
```

Der Compile-Schritt dauert ca. 2–5 Minuten.

### 3. Sprachmodell herunterladen

```bash
mkdir -p /srv/projects/linux.whisper/models
bash /srv/projects/linux.whisper/whisper.cpp/models/download-ggml-model.sh small
ln -sf /srv/projects/linux.whisper/whisper.cpp/models/ggml-small.bin /srv/projects/linux.whisper/models/ggml-small.bin
```

Das Modell `small` ist ~466 MB groß und liefert sehr gute Ergebnisse für Deutsch.

### 4. whisper-inject kompilieren

`whisper-inject` ist ein Rust-Programm, das Text über das **XDG RemoteDesktop-Portal** (EIS/libei) ins aktive Fenster tippt – ohne uinput, ohne Root-Rechte, nativ auf GNOME Wayland.

```bash
cargo build --release --manifest-path /srv/projects/linux.whisper/inject/Cargo.toml
```

### 5. Skripte ausführbar machen

```bash
chmod +x /srv/projects/linux.whisper/dictate.sh
```

### 6. whisper-inject.service als Benutzer-Dienst einrichten

```bash
mkdir -p ~/.config/systemd/user
cp /srv/projects/linux.whisper/whisper-inject.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable whisper-inject.service
systemctl --user start whisper-inject.service
```

Beim **ersten Start** erscheint ein GNOME-Dialog, der nach der Erlaubnis für Tastatureingaben fragt. Einmal bestätigen – danach startet der Daemon ohne Dialog (restore_token wird gespeichert).

Status prüfen:
```bash
systemctl --user status whisper-inject.service
```

### 7. GNOME-Tastenkürzel einrichten

```bash
gsettings set org.gnome.desktop.wm.keybindings switch-input-source "['']"
gsettings set org.gnome.desktop.wm.keybindings switch-input-source-backward "['']"

SHORTCUT="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/"
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "['$SHORTCUT']"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$SHORTCUT" name 'Whisper Diktat'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$SHORTCUT" command '/srv/projects/linux.whisper/dictate.sh'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$SHORTCUT" binding '<Super>space'
```

---

## Verwendung

| Aktion | Taste |
|---|---|
| Aufnahme starten | **Super+Space** |
| Aufnahme stoppen & Text eintippen | **Super+Space** |

Beim ersten Drücken erscheint eine Benachrichtigung „Aufnahme läuft...". Nach dem zweiten Drücken transkribiert Whisper die Aufnahme (ca. 5–30 Sekunden je nach Länge) und tippt den Text direkt ins aktive Fenster – inklusive Umlaute, ß und Sonderzeichen.

Funktioniert in Terminal-Fenstern, Texteditoren, Browsern und allen anderen Anwendungen.

---

## Modelle

Alternativ zum `small`-Modell stehen weitere zur Verfügung:

| Modell | Größe | Geschwindigkeit (CPU) | Qualität |
|---|---|---|---|
| base | 142 MB | ~5–10 s | gut |
| **small** | 466 MB | ~15–30 s | sehr gut ← Standard |
| medium | 1,5 GB | ~60 s+ | exzellent |

Modell wechseln: in `dictate.sh` die Variable `MODEL` anpassen.

---

## Sprache ändern

In `dictate.sh` ist `--language de` für Deutsch gesetzt. Für andere Sprachen einfach den Sprachcode anpassen, z. B. `--language en` für Englisch. `whisper-inject` liest die Keymap dynamisch vom XDG-Portal – es funktioniert mit jedem Tastaturlayout automatisch.

---

## Technische Hintergründe: Texteingabe auf GNOME Wayland

Die Texteingabe ist auf GNOME Wayland überraschend schwierig. Zur Dokumentation, was getestet wurde:

| Methode | Ergebnis |
|---|---|
| `wtype` | Scheitert: GNOME unterstützt `zwp_virtual_keyboard_v1` nicht |
| `ydotool type` | Funktioniert, aber Umlaute (ä, ö, ü, ß) werden falsch ausgegeben |
| `wl-copy` + Ctrl+V | Klappt in Editoren/Browsern, aber nicht im Terminal |
| `NotifyKeyboardKeysym` (XDG-Portal) | API-Aufruf kehrt ohne Fehler zurück, aber GNOME 50 tut nichts |
| **`whisper-inject` (EIS/libei)** | **Funktioniert überall** – liest Keymap vom EIS-Server, mappt Unicode auf Keycodes |

`whisper-inject` nutzt das XDG RemoteDesktop-Portal mit EIS (Emulated Input Stream / libei). Der Daemon baut beim ersten Start eine Portal-Session auf (einmaliger Dialog), speichert den `restore_token` und startet danach dialogfrei. Text wird per XKB-Keymap-Lookup auf Evdev-Keycodes abgebildet und über das EIS-Protokoll an den Compositor übergeben.

---

## Fehlerbehebung

**Kein Text wird eingetippt:**
```bash
systemctl --user status whisper-inject.service
```
Falls der Dienst nicht läuft: `systemctl --user start whisper-inject.service`

Falls der Dienst läuft aber kein Text erscheint: prüfen ob GNOME den Portal-Dialog angezeigt hat (einmalige Bestätigung notwendig). Token löschen und Dienst neu starten erzwingt den Dialog:
```bash
rm -f ~/.local/state/whisper-dictate/restore_token
systemctl --user restart whisper-inject.service
```

**Daemon-Logs ansehen:**
```bash
journalctl --user -u whisper-inject.service -n 50
```

**Aufnahme startet nicht:**
```bash
arecord -l
```
Prüfen ob ein Mikrofon aufgelistet wird.

**Shortcut reagiert nicht:**
In GNOME-Einstellungen unter *Tastatur → Tastaturkürzel → Benutzerdefinierte Tastaturkürzel* nachsehen, ob „Whisper Diktat" aufgelistet ist.
