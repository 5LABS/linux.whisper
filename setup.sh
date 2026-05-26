#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHISPER_DIR="$SCRIPT_DIR/whisper.cpp"
MODELS_DIR="$SCRIPT_DIR/models"
DICTATE_SCRIPT="$SCRIPT_DIR/dictate.sh"
INJECT_DIR="$SCRIPT_DIR/inject"
SERVICE_SRC="$SCRIPT_DIR/whisper-inject.service"
SERVICE_DST="$HOME/.config/systemd/user/whisper-inject.service"
SHORTCUT_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/"

step() { echo; echo "==> $*"; }

step "Systempakete installieren..."
sudo apt install -y git cmake gcc rustc cargo libxkbcommon-dev alsa-utils

step "whisper.cpp klonen..."
if [[ -d "$WHISPER_DIR/.git" ]]; then
    echo "Bereits vorhanden, überspringe Clone."
else
    git clone https://github.com/ggerganov/whisper.cpp "$WHISPER_DIR"
fi

step "whisper.cpp kompilieren (das dauert einige Minuten)..."
cmake -S "$WHISPER_DIR" -B "$WHISPER_DIR/build" -DCMAKE_BUILD_TYPE=Release
cmake --build "$WHISPER_DIR/build" -j"$(nproc)"

step "Whisper-Modell 'small' herunterladen (~466 MB)..."
mkdir -p "$MODELS_DIR"
if [[ -f "$MODELS_DIR/ggml-small.bin" ]]; then
    echo "Modell bereits vorhanden, überspringe Download."
else
    bash "$WHISPER_DIR/models/download-ggml-model.sh" small
    mv "$WHISPER_DIR/models/ggml-small.bin" "$MODELS_DIR/ggml-small.bin" 2>/dev/null || true
    [[ -f "$MODELS_DIR/ggml-small.bin" ]] || \
        ln -sf "$WHISPER_DIR/models/ggml-small.bin" "$MODELS_DIR/ggml-small.bin"
fi

step "whisper-inject kompilieren..."
cargo build --release --manifest-path "$INJECT_DIR/Cargo.toml"

step "hold_daemon kompilieren..."
if [[ ! -f "$SCRIPT_DIR/hold_daemon" ]]; then
    gcc -O2 -o "$SCRIPT_DIR/hold_daemon" "$SCRIPT_DIR/hold_daemon.c"
fi

step "Benutzer zur Gruppe 'input' hinzufügen (für hold_daemon)..."
if ! groups | grep -q '\binput\b'; then
    sudo usermod -aG input "$USER"
    echo "  Benutzer zur Gruppe 'input' hinzugefügt."
    echo "  WICHTIG: Bitte ab- und wieder einloggen, damit die Gruppenänderung wirksam wird."
else
    echo "  Bereits Mitglied der Gruppe 'input'."
fi

step "Skripte ausführbar machen..."
chmod +x "$DICTATE_SCRIPT"

step "whisper-inject.service als Benutzer-Systemd-Dienst installieren..."
mkdir -p "$HOME/.config/systemd/user"
cp "$SERVICE_SRC" "$SERVICE_DST"
systemctl --user daemon-reload
systemctl --user enable whisper-inject.service
systemctl --user start whisper-inject.service || true
echo "  Hinweis: Beim ersten Start erscheint ein Portal-Dialog zur Bestätigung."
echo "  whisper-inject.service Status prüfen mit:"
echo "    systemctl --user status whisper-inject.service"

step "Standard-Binding für Super+Space entfernen (Eingabequelle wechseln)..."
gsettings set org.gnome.desktop.wm.keybindings switch-input-source "['']"
gsettings set org.gnome.desktop.wm.keybindings switch-input-source-backward "['']"

step "GNOME-Tastenkürzel Super+Space → dictate.sh setzen..."
EXISTING=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
if echo "$EXISTING" | grep -q "custom0"; then
    echo "Shortcut custom0 bereits vorhanden, aktualisiere..."
else
    if [[ "$EXISTING" == "@as []" ]] || [[ "$EXISTING" == "[]" ]]; then
        gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings \
            "['$SHORTCUT_PATH']"
    else
        CLEANED="${EXISTING%]}"
        gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings \
            "${CLEANED}, '$SHORTCUT_PATH']"
    fi
fi

gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$SHORTCUT_PATH" \
    name 'Whisper Diktat'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$SHORTCUT_PATH" \
    command "$DICTATE_SCRIPT"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$SHORTCUT_PATH" \
    binding '<Super>space'

step "Fertig!"
echo
echo "  Taste:    Super+Space (einmal drücken = Aufnahme starten,"
echo "            nochmal drücken = stoppen & transkribieren)"
echo "  Skript:   $DICTATE_SCRIPT"
echo "  Modell:   $MODELS_DIR/ggml-small.bin"
echo "  Daemon:   whisper-inject.service (XDG RemoteDesktop-Portal + EIS)"
echo
echo "  Tipp: dictate.sh manuell testen mit:"
echo "    $DICTATE_SCRIPT   # Aufnahme starten"
echo "    $DICTATE_SCRIPT   # Aufnahme stoppen + tippen"
