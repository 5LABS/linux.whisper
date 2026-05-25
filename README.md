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
sudo apt install -y git cmake gcc ydotool
```

### 2. udev-Regel für ydotool anlegen

```bash
echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' | sudo tee /etc/udev/rules.d/80-uinput.rules && sudo udevadm control --reload-rules && sudo udevadm trigger
```

### 3. Benutzer zur Gruppe `input` hinzufügen

```bash
sudo usermod -aG input $USER
```

Danach **ausloggen und wieder einloggen** (einmalig nötig).

### 4. whisper.cpp klonen und kompilieren

```bash
cd /srv/projects/linux.whisper
git clone https://github.com/ggerganov/whisper.cpp
cmake -S whisper.cpp -B whisper.cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build whisper.cpp/build -j$(nproc)
```

Der Compile-Schritt dauert ca. 2–5 Minuten.

### 5. Sprachmodell herunterladen

```bash
mkdir -p /srv/projects/linux.whisper/models
bash /srv/projects/linux.whisper/whisper.cpp/models/download-ggml-model.sh small
ln -sf /srv/projects/linux.whisper/whisper.cpp/models/ggml-small.bin /srv/projects/linux.whisper/models/ggml-small.bin
```

Das Modell `small` ist ~466 MB groß und liefert sehr gute Ergebnisse für Deutsch.

### 6. type_de kompilieren

`type_de` ist ein kleines C-Programm, das Whisper-Text zeichenweise über ydotool eintippt und dabei deutsche Umlaute und Sonderzeichen korrekt auf QWERTZ-Keycodes abbildet.

```bash
gcc -O2 -o /srv/projects/linux.whisper/type_de /srv/projects/linux.whisper/type_de.c
```

### 7. Skripte ausführbar machen

```bash
chmod +x /srv/projects/linux.whisper/dictate.sh
```

### 8. ydotoold als Hintergrunddienst einrichten

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/ydotoold.service << 'EOF'
[Unit]
Description=ydotool daemon

[Service]
ExecStart=/usr/bin/ydotoold
Restart=always

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload && systemctl --user enable ydotoold && systemctl --user start ydotoold
```

### 9. GNOME-Tastenkürzel einrichten

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

Beim ersten Drücken erscheint eine Benachrichtigung „Aufnahme läuft...". Nach dem zweiten Drücken transkribiert Whisper die Aufnahme (ca. 5–30 Sekunden je nach Länge) und tippt den Text über `type_de` direkt ins aktive Fenster – inklusive Umlaute, ß und Sonderzeichen.

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

In `dictate.sh` ist `--language de` für Deutsch gesetzt. Für andere Sprachen einfach den Sprachcode anpassen, z. B. `--language en` für Englisch. Das Keymap in `type_de.c` ist auf deutsches QWERTZ ausgelegt – bei anderen Sprachen ggf. anpassen und neu kompilieren.

---

## Technische Hintergründe: Texteingabe auf GNOME Wayland

Die Texteingabe ist auf GNOME Wayland überraschend schwierig. Zur Dokumentation, was getestet wurde:

| Methode | Ergebnis |
|---|---|
| `wtype` | Scheitert: GNOME unterstützt `zwp_virtual_keyboard_v1` nicht |
| `ydotool type` | Funktioniert, aber Umlaute (ä, ö, ü, ß) werden falsch ausgegeben |
| `wl-copy` + Ctrl+V | Klappt in Editoren/Browsern, aber nicht im Terminal (dort ist Ctrl+Shift+V nötig) |
| `type_de` + `ydotool key` | Funktioniert überall – mappt UTF-8 explizit auf QWERTZ-Keycodes |

`type_de` verwendet einen Key-Delay von 20ms, damit Terminal-Eingabepuffer keine Zeichen (insbesondere Leerzeichen) verschlucken.

---

## Fehlerbehebung

**Kein Text wird eingetippt:**
```bash
systemctl --user status ydotoold
```
Falls der Dienst nicht läuft: `systemctl --user start ydotoold`

**`type_de` fehlt oder ist nicht ausführbar:**
```bash
gcc -O2 -o /srv/projects/linux.whisper/type_de /srv/projects/linux.whisper/type_de.c
chmod +x /srv/projects/linux.whisper/type_de
```

**Aufnahme startet nicht:**
```bash
arecord -l
```
Prüfen ob ein Mikrofon aufgelistet wird.

**Shortcut reagiert nicht:**
In GNOME-Einstellungen unter *Tastatur → Tastaturkürzel → Benutzerdefinierte Tastenkürzel* nachsehen, ob „Whisper Diktat" aufgelistet ist.
