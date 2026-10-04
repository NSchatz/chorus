#!/usr/bin/env python3
"""Writes music.wav: four seconds of synthesized music, 16 kHz mono 16-bit.

chorus's own work (MIT OR Apache-2.0), made from arithmetic alone so that no
recording's licence is in question: a four-chord progression of plucked notes
with harmonics, a bass line and a noise hi-hat. Standard library only:

    python3 fixtures/wakeword/make-music.py
"""
import math
import pathlib
import struct
import wave

RATE = 16000
SECONDS = 4
# A minor, F major, C major, G major: root and the chord's three notes, in Hz.
CHORDS = [
    (110.00, (220.00, 261.63, 329.63)),
    (87.31, (174.61, 220.00, 261.63)),
    (130.81, (261.63, 329.63, 392.00)),
    (98.00, (196.00, 246.94, 293.66)),
]


def pluck(freq, t):
    """A note struck at t = 0: six harmonics, the higher ones dying sooner."""
    return sum(math.sin(2 * math.pi * freq * h * t) * math.exp(-t * (2.0 + 1.5 * h)) / h for h in range(1, 7))


def main():
    samples = []
    noise = 12345
    beat = RATE // 4  # sixteen beats in four seconds
    for n in range(RATE * SECONDS):
        chord = CHORDS[(n // (RATE * SECONDS // len(CHORDS))) % len(CHORDS)]
        root, notes = chord
        k = n // beat
        t = (n % beat) / RATE
        v = 0.5 * pluck(notes[k % 3], t) + 0.25 * pluck(notes[(k + 1) % 3] * 2, t)
        v += 0.6 * math.sin(2 * math.pi * root * n / RATE) * math.exp(-t * 3.0)
        # A linear congruential generator, so the hi-hat is the same on every machine.
        noise = (noise * 1103515245 + 12345) & 0x7FFFFFFF
        if k % 2 == 1:
            v += 0.15 * (noise / 0x40000000 - 1.0) * math.exp(-t * 40.0)
        samples.append(max(-32767, min(32767, round(v * 9000))))
    out = pathlib.Path(__file__).with_name("music.wav")
    with wave.open(str(out), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(struct.pack(f"<{len(samples)}h", *samples))


if __name__ == "__main__":
    main()
