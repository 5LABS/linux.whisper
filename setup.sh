#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHISPER_DIR="$SCRIPT_DIR/whisper.cpp"
MODELS_DIR="$SCRIPT_DIR/models"
DICTATE_SCRIPT="$SCRIPT_DIR/dictate.sh"
SHORTCUT_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/"

step() { echo; echo "==> $*"; }

step "Systempakete installieren..."
sudo apt install -y git cmake xdotool

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
    # Falls das Modell direkt im whisper.cpp/models-Ordner bleibt
    [[ -f "$MODELS_DIR/ggml-small.bin" ]] || \
        ln -sf "$WHISPER_DIR/models/ggml-small.bin" "$MODELS_DIR/ggml-small.bin"
fi

step "dictate.sh ausführbar machen..."
chmod +x "$DICTATE_SCRIPT"

step "Standard-Binding für Super+Space entfernen (Eingabequelle wechseln)..."
gsettings set org.gnome.desktop.wm.keybindings switch-input-source "['']"
gsettings set org.gnome.desktop.wm.keybindings switch-input-source-backward "['']"

step "GNOME-Tastenkürzel Super+Space → dictate.sh setzen..."
# Bestehende Custom-Bindings lesen und ergänzen
EXISTING=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
if echo "$EXISTING" | grep -q "custom0"; then
    echo "Shortcut custom0 bereits vorhanden, aktualisiere..."
else
    if [[ "$EXISTING" == "@as []" ]] || [[ "$EXISTING" == "[]" ]]; then
        gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings \
            "['$SHORTCUT_PATH']"
    else
        # Bestehende Einträge behalten und custom0 hinzufügen
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
echo "  Text:     wird direkt ins aktive Fenster getippt"
echo "            + in Zwischenablage kopiert"
echo
echo "  Tipp: dictate.sh manuell testen mit:"
echo "    $DICTATE_SCRIPT   # Aufnahme starten"
echo "    $DICTATE_SCRIPT   # Aufnahme stoppen + tippen"
