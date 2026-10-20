#!/usr/bin/env python3
"""Renders the README's screenshots into docs/images with the app's own
offscreen snapshot mode (`das-meter --render-snapshot`), then labels and
combines them. Build first: `cargo build --release --bin das-meter`.
Needs Pillow (`pip install pillow`)."""

import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(ROOT, "target", "release", "das-meter")
OUT = os.path.join(ROOT, "docs", "images")
FONTS = os.path.join(ROOT, "crates", "app", "assets", "fonts")
WORK = tempfile.mkdtemp(prefix="das-meter-shots-")

LABEL = ImageFont.truetype(os.path.join(FONTS, "Geist-SemiBold.otf"), 30)
SMALL = ImageFont.truetype(os.path.join(FONTS, "Geist-Regular.otf"), 24)
INK, PAPER = (236, 236, 244), (14, 14, 20)


def read_pam(path):
    with open(path, "rb") as f:
        data = f.read()
    head, body = data.split(b"ENDHDR\n", 1)
    fields = dict(
        line.split(b" ", 1) for line in head.split(b"\n")[1:] if b" " in line
    )
    w, h = int(fields[b"WIDTH"]), int(fields[b"HEIGHT"])
    return Image.frombytes("RGBA", (w, h), body[: w * h * 4])


def shot(option, theme=None, style=None, override=None, classic=False):
    """One snapshot, as an RGBA image."""
    env = dict(os.environ, HOME=os.path.join(WORK, "home"))
    for key in ("DASMETER_THEME", "DASMETER_STYLE", "DASMETER_OVERRIDE", "DASMETER_CLASSIC"):
        env.pop(key, None)
    if theme:
        env["DASMETER_THEME"] = theme
    if style:
        env["DASMETER_STYLE"] = style
    if override:
        env["DASMETER_OVERRIDE"] = override
    if classic:
        env["DASMETER_CLASSIC"] = "1"
    path = os.path.join(WORK, "shot.pam")
    subprocess.run([APP, "--render-snapshot", path, option], env=env, check=True,
                   stdout=subprocess.DEVNULL)
    return read_pam(path)


def flat(image):
    """An RGBA snapshot over the app's own dark backdrop (what's under a
    see-through window on a plain desktop)."""
    base = Image.new("RGBA", image.size, PAPER + (255,))
    return Image.alpha_composite(base, image).convert("RGB")


def width(image, w):
    if image.width == w:
        return image
    return image.resize((w, round(image.height * w / image.width)), Image.LANCZOS)


def labelled(images, labels, columns=None, tile_width=None, gap=16, caption_height=56):
    """Images in a grid, each with its label underneath."""
    columns = columns or len(images)
    if tile_width:
        images = [width(i, tile_width) for i in images]
    tw = max(i.width for i in images)
    th = max(i.height for i in images)
    rows = (len(images) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * tw + (columns + 1) * gap,
                              rows * (th + caption_height) + (rows + 1) * gap), PAPER)
    draw = ImageDraw.Draw(sheet)
    for n, (image, label) in enumerate(zip(images, labels)):
        x = gap + (n % columns) * (tw + gap)
        y = gap + (n // columns) * (th + caption_height + gap)
        sheet.paste(image, (x, y))
        title, _, note = label.partition("\n")
        draw.text((x + 4, y + th + 10), title, font=LABEL, fill=INK)
        if note:
            tx = x + 12 + draw.textlength(title, font=LABEL)
            draw.text((tx, y + th + 15), note, font=SMALL, fill=(150, 150, 165))
    return sheet


def save(image, name, w=None):
    image = image.convert("RGB")
    if w:
        image = width(image, w)
    path = os.path.join(OUT, name)
    image.save(path, "PNG", optimize=True)
    print(f"{name:32s} {image.size[0]}x{image.size[1]}  {os.path.getsize(path) // 1024} KB")


def desktop(size):
    """A colourful stand-in for a desktop picture, to show see-through."""
    w, h = size
    img = Image.new("RGB", size)
    px = img.load()
    for y in range(h):
        for x in range(w):
            u, v = x / w, y / h
            px[x, y] = (int(40 + 160 * u), int(60 + 90 * (1 - v)), int(150 + 90 * v * (1 - u)))
    return img


def main():
    os.makedirs(OUT, exist_ok=True)

    # The overview, in both Looks.
    save(flat(shot("showcase")), "smooth.png", 1600)
    save(flat(shot("showcase", classic=True)), "classic.png", 1600)
    save(flat(shot("showcase-bar")), "bar.png", 1600)
    save(flat(shot("zoom")).crop((0, 0, 2400, 716)), "spectrum-zoom.png", 1600)

    # Every Meter alone.
    for kind in ["waveform", "spectrum", "spectrogram", "loudness", "stereometer",
                 "phase-scope", "cepstrum", "oscilloscope"]:
        save(flat(shot(f"solo-{kind}")), f"meter-{kind}.png", 960)

    # Every built-in Theme, plus one made from scratch.
    themes = ["Nocturne", "Dark", "Light", "High contrast", "Midnight Purple",
              "Deep Turquoise", "Graphite"]
    tiles = [flat(shot("showcase", theme=t)) for t in themes]
    tiles.append(flat(shot("showcase", theme="Nocturne",
                           style="gap=14,radius=22,glow=1,vignette=0.8,gradient=1")))
    save(labelled(tiles, themes + ["Your own\n(edited Nocturne)"], columns=2,
                  tile_width=760), "themes.png", 1600)

    # The Theme editor.
    save(flat(shot("theme-settings")), "theme-editor.png", 1400)

    # What each styling setting does.
    spectrum = "solo-spectrum"
    save(labelled([flat(shot(spectrum)), flat(shot(spectrum, style="look=classic"))],
                  ["Look: Smooth", "Look: Classic"], tile_width=760), "setting-look.png", 1600)
    save(labelled([flat(shot(spectrum, style=f"glow={g}")) for g in (0, 0.6, 1)],
                  ["Glow 0", "Glow 0.6\n(default)", "Glow 1"], tile_width=520),
         "setting-glow.png", 1600)
    save(labelled([flat(shot(spectrum, style=f"fade={f}")) for f in (0, 0.4)],
                  ["Grid fade 0", "Grid fade 0.4"], tile_width=760), "setting-grid-fade.png", 1600)
    save(labelled([flat(shot(spectrum, style="gradient=0,vignette=0")),
                   flat(shot(spectrum, style="gradient=1,vignette=1"))],
                  ["Gradient and Vignette off", "Gradient and Vignette full"], tile_width=760),
         "setting-gradient-vignette.png", 1600)
    osc = "solo-oscilloscope"
    # Close up, where the difference shows.
    save(labelled([flat(shot(osc, style=f"line={l}")).crop((260, 60, 760, 460))
                   for l in ("thin", "normal", "thick")],
                  ["Lines: Thin", "Normal", "Thick"]), "setting-lines.png", 1600)
    loud = "solo-loudness"
    save(labelled([flat(shot(loud, style=f"text={t}")) for t in (0.9, 1.5)],
                  ["Text size 0.9", "Text size 1.5"], tile_width=760), "setting-text-size.png", 1600)
    save(labelled([flat(shot("showcase", style="gap=0,radius=0")),
                   flat(shot("showcase", style="gap=16,radius=24"))],
                  ["Gap 0, corner radius 0", "Gap 16, corner radius 24"], tile_width=760),
         "setting-gap-radius.png", 1600)
    # See-through: the Bar over a desktop picture.
    bars = []
    for opacity in (1.0, 0.15):
        bar = shot("showcase-bar", style=f"opacity={opacity}")
        backdrop = desktop(bar.size).convert("RGBA")
        bars.append(Image.alpha_composite(backdrop, bar).convert("RGB"))
    save(labelled(bars, ["Background opacity 1", "Background opacity 0.15"], columns=1,
                  tile_width=1560), "setting-opacity.png", 1600)
    # One Meter's own colours over its Theme's.
    save(labelled([flat(shot("showcase")),
                   flat(shot("showcase", override="waveform_low=#3ddc97,waveform_mid=#4dc3ff,waveform_high=#ffffff"))],
                  ["Theme colours", "The Waveform with its own colours"], tile_width=760),
         "setting-meter-colours.png", 1600)


if __name__ == "__main__":
    sys.exit(main())
