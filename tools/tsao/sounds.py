"""TSAO sounds for the engine: effects `Data/SOUNDS/WAV/*.wav` are Microsoft ADPCM, the
client plays PCM -> `wav/<id>.wav` 16-bit PCM; map music `Data/SOUNDS/MP3/<n>.mp3` (what
the TSAO client played, `Audio.MP3_Play`) -> `OGG/ost_<n>.ogg`.

usage: python -I sounds.py <tsao_root> <out_root> [ffmpeg]
"""
import os
import subprocess
import sys

FFMPEG = r"C:\Program Files\Shotcut\ffmpeg.exe"


def run(ffmpeg, src, dst, args):
    r = subprocess.run([ffmpeg, "-v", "error", "-y", "-i", src, *args, dst], capture_output=True)
    return r.returncode == 0


def main(src, out, ffmpeg=FFMPEG):
    snd = os.path.join(src, "Cliente", "Data", "SOUNDS")
    wav, ogg = os.path.join(out, "wav"), os.path.join(out, "OGG")
    os.makedirs(wav, exist_ok=True)
    os.makedirs(ogg, exist_ok=True)
    ok = bad = 0
    for f in os.listdir(os.path.join(snd, "WAV")):
        if f.lower().endswith(".wav"):
            if run(ffmpeg, os.path.join(snd, "WAV", f), os.path.join(wav, f.lower()), ["-c:a", "pcm_s16le"]):
                ok += 1
            else:
                bad += 1
    # Rain loops: TSAO names them, the engine plays them by number (fx.rs SND_RAIN_*).
    import shutil
    for name, n in (("lluviain.wav", 191), ("lluviaout.wav", 194)):
        if os.path.exists(os.path.join(wav, name)):
            shutil.copyfile(os.path.join(wav, name), os.path.join(wav, f"{n}.wav"))
    music = 0
    for f in os.listdir(os.path.join(snd, "MP3")):
        n, ext = os.path.splitext(f)
        if ext.lower() == ".mp3" and n.isdigit():
            if run(ffmpeg, os.path.join(snd, "MP3", f), os.path.join(ogg, f"ost_{n}.ogg"), ["-c:a", "libvorbis", "-q:a", "5"]):
                music += 1
            else:
                bad += 1
    print(f"{ok} effects, {music} music tracks, {bad} failed")


if __name__ == "__main__":
    main(*sys.argv[1:])
