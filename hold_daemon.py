#!/usr/bin/env python3
import asyncio
import subprocess
import os
from evdev import InputDevice, ecodes, list_devices

SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dictate.sh")
SUPER_KEYS = {ecodes.KEY_LEFTMETA, ecodes.KEY_RIGHTMETA}
SPACE = ecodes.KEY_SPACE


def find_keyboards():
    keyboards = []
    for path in list_devices():
        dev = InputDevice(path)
        caps = dev.capabilities()
        keys = caps.get(ecodes.EV_KEY, [])
        if ecodes.KEY_SPACE in keys and ecodes.KEY_LEFTMETA in keys:
            keyboards.append(dev)
    return keyboards


async def monitor(devices):
    super_held = False
    recording = False

    async def read(dev):
        nonlocal super_held, recording
        async for ev in dev.async_read_loop():
            if ev.type != ecodes.EV_KEY:
                continue
            if ev.code in SUPER_KEYS:
                super_held = ev.value == 1
                if ev.value == 0 and recording:
                    recording = False
                    for d in devices:
                        try:
                            d.ungrab()
                        except Exception:
                            pass
                    subprocess.Popen([SCRIPT, "stop"])
            elif ev.code == SPACE:
                if ev.value == 1 and super_held and not recording:
                    recording = True
                    for d in devices:
                        try:
                            d.grab()
                        except Exception:
                            pass
                    subprocess.Popen([SCRIPT, "start"])
                elif ev.value == 0 and recording:
                    recording = False
                    for d in devices:
                        try:
                            d.ungrab()
                        except Exception:
                            pass
                    subprocess.Popen([SCRIPT, "stop"])

    await asyncio.gather(*[read(d) for d in devices])


if __name__ == "__main__":
    devs = find_keyboards()
    if not devs:
        raise SystemExit("Kein Keyboard-Gerät gefunden")
    asyncio.run(monitor(devs))
