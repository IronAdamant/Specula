"""Makes the Vorbis decoder's test fixtures from signals generated here (no outside audio).

Run from this folder: python make_fixtures.py
Needs ffmpeg with libvorbis on PATH. Each fixture is:
  <name>.ogg      encoded by ffmpeg (libvorbis, or ffmpeg's own Vorbis encoder for *_ffenc)
  <name>.ref.wav  ffmpeg's own decode of that .ogg, 16-bit PCM (the reference)
The decoder must match the reference within the tolerance in tests/vorbis.rs.
Made 2026-11-06 with ffmpeg 9.0.2 (its libvorbis encoder and its native decoder).
"""
import math
import os
import random
import struct
import subprocess
import wave


def write_wav(path, rate, chans):
    frames = len(chans[0])
    with wave.open(path, "wb") as w:
        w.setnchannels(len(chans))
        w.setsampwidth(2)
        w.setframerate(rate)
        data = bytearray()
        for i in range(frames):
            for c in chans:
                data += struct.pack("<h", max(-32767, min(32767, int(round(c[i] * 32767)))))
        w.writeframes(bytes(data))


def sweep(rate, secs, f0, f1, amp):
    n = int(rate * secs)
    k = math.log(f1 / f0)
    return [amp * math.sin(2 * math.pi * f0 * secs / k * (math.exp(k * i / n) - 1)) for i in range(n)]


def make(name, rate, chans, codec_args):
    src = name + ".src.wav"
    write_wav(src, rate, chans)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", src, *codec_args, name + ".ogg"], check=True)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", name + ".ogg", "-c:a", "pcm_s16le", name + ".ref.wav"], check=True)
    os.remove(src)


rnd = random.Random(7)
# 1. A mono log sweep at 22.05 kHz, medium quality.
make("sweep_mono_22k", 22050, [sweep(22050, 1.0, 50, 10000, 0.5)], ["-c:a", "libvorbis", "-q:a", "2"])
# 2. Stereo at 48 kHz: a tone and noise on the left, a chord on the right, a silent gap and
#    clicks (transients force short blocks), quality 5.
n = int(48000 * 0.6)
left, right = [], []
for i in range(n):
    t = i / 48000
    gap = 0.25 < t < 0.32
    click = (i % 9000) < 30
    l = 0.0 if gap else 0.3 * math.sin(2 * math.pi * 440 * t) + 0.1 * (rnd.random() * 2 - 1)
    r = 0.0 if gap else 0.2 * (math.sin(2 * math.pi * 330 * t) + math.sin(2 * math.pi * 415.3 * t) + math.sin(2 * math.pi * 495 * t))
    if click:
        l += 0.6
        r -= 0.6
    left.append(l)
    right.append(r)
make("stereo_48k", 48000, [left, right], ["-c:a", "libvorbis", "-q:a", "5"])
# 3. White-noise bursts, mono 44.1 kHz, the lowest quality.
n = int(44100 * 0.5)
make("noise_44k_q0", 44100, [[(rnd.random() * 2 - 1) * (0.5 if (i // 4410) % 2 == 0 else 0.05) for i in range(n)]], ["-c:a", "libvorbis", "-q:a", "0"])
# 4. Six channels (5.1) at 48 kHz: a different tone per channel.
n = int(48000 * 0.2)
make("surround_6ch", 48000, [[0.2 * math.sin(2 * math.pi * (200 + 150 * c) * i / 48000) for i in range(n)] for c in range(6)], ["-c:a", "libvorbis", "-q:a", "3"])
# 5. ffmpeg's own Vorbis encoder (different codebooks and residue setup), stereo 44.1 kHz.
n = int(44100 * 0.4)
make("stereo_44k_ffenc", 44100, [sweep(44100, 0.4, 100, 8000, 0.4), [0.3 * math.sin(2 * math.pi * 250 * i / 44100) for i in range(n)]], ["-c:a", "vorbis", "-strict", "-2", "-ac", "2"])
# 6. High quality stereo at 32 kHz with near-full-scale content.
n = int(32000 * 0.4)
make("loud_32k_q10", 32000, [[0.9 * math.sin(2 * math.pi * 1000 * i / 32000) for i in range(n)], [0.9 * math.sin(2 * math.pi * 1500 * i / 32000 + 1) for i in range(n)]], ["-c:a", "libvorbis", "-q:a", "10"])
