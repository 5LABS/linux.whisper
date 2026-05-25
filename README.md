# Whisper Diktat – Spracherkennung per Tastendruck

Halte **Super+Space** gedrückt, sprich, lass los – der erkannte Text wird direkt ins aktive Fenster getippt.

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
sudo apt install -y git cmake ydotool wl-clipboard
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

### 6. Skripte ausführbar machen

```bash
chmod +x /srv/projects/linux.whisper/dictate.sh
```

### 7. ydotoold als Hintergrunddienst einrichten

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

### 8. Input-Source-Switching deaktivieren und Hold-Daemon starten

```bash
gsettings set org.gnome.desktop.wm.keybindings switch-input-source "['']"
gsettings set org.gnome.desktop.wm.keybindings switch-input-source-backward "['']"

cp /srv/projects/linux.whisper/whisper-hold.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now whisper-hold
```

Der `whisper-hold`-Daemon liest Tastatureingaben direkt über evdev und startet/stoppt die Aufnahme bei Drücken bzw. Loslassen von Super+Space. Kein GNOME-Tastenkürzel nötig.

---

## Verwendung

| Aktion | Taste |
|---|---|
| Aufnahme starten | **Super+Space** (halten) |
| Aufnahme stoppen & Text eintippen | **Super+Space** (loslassen) |

Beim Drücken erscheint eine Benachrichtigung „Aufnahme läuft...". Nach dem Stoppen transkribiert Whisper die Aufnahme (ca. 5–30 Sekunden je nach Länge) und fügt den Text ins aktive Fenster ein:

- **Kurze Texte** (≤ 200 Zeichen): direkt getippt via ydotool – die Zwischenablage wird nicht angefasst.
- **Lange Texte** (> 200 Zeichen): über Zwischenablage + Ctrl+V eingefügt; der vorherige Clipboard-Inhalt wird danach wiederhergestellt.

Umlaute und Sonderzeichen funktionieren in beiden Varianten korrekt. Hinweis: In Terminal-Fenstern funktioniert Ctrl+V nicht (dort ist Ctrl+Shift+V nötig) – die Clipboard-Variante greift daher am besten in Texteditoren und Browser-Textfeldern.

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

In `dictate.sh` ist `-l de` für Deutsch gesetzt. Für andere Sprachen einfach den Sprachcode anpassen, z. B. `-l en` für Englisch.

---

## Fehlerbehebung

**Kein Text wird eingetippt:**
```bash
systemctl --user status ydotoold
```
Falls der Dienst nicht läuft: `systemctl --user start ydotoold`

**Aufnahme startet nicht:**
```bash
arecord -l
```
Prüfen ob ein Mikrofon aufgelistet wird.

**Shortcut reagiert nicht:**
In GNOME-Einstellungen unter *Tastatur → Tastaturkürzel → Benutzerdefinierte Tastenkürzel* nachsehen, ob „Whisper Diktat" aufgelistet ist.
