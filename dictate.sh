#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHISPER_BIN="$SCRIPT_DIR/whisper.cpp/build/bin/whisper-cli"
MODEL="$SCRIPT_DIR/models/ggml-small.bin"
AUDIO_FILE="/tmp/whisper_audio.wav"
PID_FILE="/tmp/whisper_dictate.pid"
LOCK_FILE="/tmp/whisper_dictate.lock"

notify() {
    notify-send --app-name="Whisper Diktat" --expire-time=3000 "$1" "$2" 2>/dev/null || true
}

trap 'rm -f "$LOCK_FILE"' EXIT

# Transkription läuft noch (Lock ohne PID = busy)
if [[ -f "$LOCK_FILE" ]] && [[ ! -f "$PID_FILE" ]]; then
    notify "Whisper Diktat" "Transkription läuft noch..."
    exit 0
fi

if [[ -f "$PID_FILE" ]]; then
    PID=$(cat "$PID_FILE")
    rm -f "$PID_FILE"

    if kill -0 "$PID" 2>/dev/null; then
        kill "$PID"
        sleep 0.3
    fi

    if [[ ! -f "$AUDIO_FILE" ]] || [[ ! -s "$AUDIO_FILE" ]]; then
        notify "Whisper Diktat" "Keine Audiodaten aufgenommen."
        rm -f "$LOCK_FILE"
        exit 0
    fi

    notify "Whisper Diktat" "Transkribiere..."

    TEXT=$("$WHISPER_BIN" \
        --model "$MODEL" \
        --language de \
        --no-timestamps \
        --file "$AUDIO_FILE" \
        2>/dev/null \
        | sed 's/^\[.*\]//;s/^([^)]*)\s*$//' \
        | grep -iv '^\s*\(\(musik\)\|\[music\]\|\[blank_audio\]\|\.\.\.\|♪\)\s*$' \
        | tr '\n' ' ' \
        | tr -s ' ' \
        | sed 's/^ //;s/ $//' \
        || true)

    rm -f "$AUDIO_FILE"

    if [[ -z "$TEXT" ]]; then
        notify "Whisper Diktat" "Kein Text erkannt."
        rm -f "$LOCK_FILE"
        exit 0
    fi

    OLD_CLIP=$(wl-paste --no-newline 2>/dev/null || true)
    printf '%s' "$TEXT" | wl-copy
    sleep 0.15
    YDOTOOL_SOCKET="/run/user/$(id -u)/.ydotool_socket" ydotool key 29:1 47:1 47:0 29:0
    sleep 0.2
    if [[ -n "$OLD_CLIP" ]]; then
        printf '%s' "$OLD_CLIP" | wl-copy
    fi

    notify "Whisper Diktat" "$TEXT"
    rm -f "$LOCK_FILE"
else
    rm -f "$AUDIO_FILE"
    touch "$LOCK_FILE"
    arecord -f S16_LE -r 16000 -c 1 -q "$AUDIO_FILE" &
    echo $! > "$PID_FILE"
    notify "Whisper Diktat" "🎤 Aufnahme läuft... (Super+Space zum Stoppen)"
fi
