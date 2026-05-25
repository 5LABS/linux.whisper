#!/usr/bin/env python3
"""Tippt Text über ydotool mit korrektem deutschen (QWERTZ) Tastenlayout.

Liest Text von stdin und sendet die passenden physischen Tastencodes
(evdev) inklusive Modifier an ydotool. Die Zwischenablage wird nicht berührt.
"""
import os
import subprocess
import sys

# evdev-Keycodes (siehe /usr/include/linux/input-event-codes.h)
SHIFT = 42
ALTGR = 100

# Zeichen -> (keycode, modifier|None) für deutsches Layout
KEYMAP = {
    # Buchstaben ohne Modifier (y/z gegenüber US vertauscht)
    "a": (30, None), "b": (48, None), "c": (46, None), "d": (32, None),
    "e": (18, None), "f": (33, None), "g": (34, None), "h": (35, None),
    "i": (23, None), "j": (36, None), "k": (37, None), "l": (38, None),
    "m": (50, None), "n": (49, None), "o": (24, None), "p": (25, None),
    "q": (16, None), "r": (19, None), "s": (31, None), "t": (20, None),
    "u": (22, None), "v": (47, None), "w": (17, None), "x": (45, None),
    "y": (44, None), "z": (21, None),
    "ä": (40, None), "ö": (39, None), "ü": (26, None), "ß": (12, None),
    # Ziffern (im deutschen Layout ohne Shift)
    "1": (2, None), "2": (3, None), "3": (4, None), "4": (5, None),
    "5": (6, None), "6": (7, None), "7": (8, None), "8": (9, None),
    "9": (10, None), "0": (11, None),
    # Whitespace
    " ": (57, None), "\t": (15, None),
    # Satzzeichen ohne Modifier
    ".": (52, None), ",": (51, None), "-": (53, None),
    "+": (27, None), "#": (43, None), "<": (86, None),
    # Satzzeichen mit Shift
    "!": (2, SHIFT), '"': (3, SHIFT), "§": (4, SHIFT), "$": (5, SHIFT),
    "%": (6, SHIFT), "&": (7, SHIFT), "/": (8, SHIFT), "(": (9, SHIFT),
    ")": (10, SHIFT), "=": (11, SHIFT), "?": (12, SHIFT), "*": (27, SHIFT),
    "'": (43, SHIFT), ";": (51, SHIFT), ":": (52, SHIFT), "_": (53, SHIFT),
    ">": (86, SHIFT),
    # AltGr-Zeichen
    "@": (16, ALTGR), "€": (18, ALTGR), "{": (8, ALTGR), "[": (9, ALTGR),
    "]": (10, ALTGR), "}": (11, ALTGR), "\\": (12, ALTGR), "~": (27, ALTGR),
    "|": (86, ALTGR),
}

# Großbuchstaben = Kleinbuchstabe + Shift
for _ch in "abcdefghijklmnopqrstuvwxyzäöü":
    _kc, _ = KEYMAP[_ch]
    KEYMAP[_ch.upper()] = (_kc, SHIFT)

# Typografische Zeichen auf Tastatur-Äquivalente normalisieren
NORMALIZE = {
    "„": '"', "“": '"', "”": '"', "»": '"', "«": '"',
    "‚": "'", "‘": "'", "’": "'",
    "–": "-", "—": "-", "…": "...",
    " ": " ",  # geschütztes Leerzeichen
}


def build_args(text):
    args = []
    for ch in text:
        ch = NORMALIZE.get(ch, ch)
        if len(ch) > 1:  # z.B. "…" -> "..."
            for sub in ch:
                args.extend(_char_args(sub))
            continue
        args.extend(_char_args(ch))
    return args


def _char_args(ch):
    entry = KEYMAP.get(ch)
    if entry is None:
        return []  # unbekanntes Zeichen überspringen
    keycode, mod = entry
    seq = []
    if mod is not None:
        seq.append(f"{mod}:1")
    seq.append(f"{keycode}:1")
    seq.append(f"{keycode}:0")
    if mod is not None:
        seq.append(f"{mod}:0")
    return seq


def main():
    text = sys.stdin.read()
    if not text:
        return
    args = build_args(text)
    if not args:
        return
    socket = os.environ.get(
        "YDOTOOL_SOCKET", f"/run/user/{os.getuid()}/.ydotool_socket"
    )
    env = dict(os.environ, YDOTOOL_SOCKET=socket)
    # In Blöcken senden, damit die Argumentliste nicht zu lang wird
    chunk = 400
    for i in range(0, len(args), chunk):
        subprocess.run(
            ["ydotool", "key", "--key-delay", "6", *args[i : i + chunk]],
            env=env,
            check=True,
        )


if __name__ == "__main__":
    main()
