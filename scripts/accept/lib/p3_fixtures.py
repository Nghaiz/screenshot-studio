"""Fixture library for phase 3.4's Accept (scripts/accept/p3.mjs; plan phase-3.4 step 2, README D10).

usage: <sidecar-venv>\\Scripts\\python.exe -I p3_fixtures.py <empty out dir>

Writes 50 generated images plus two planted duplicates and prints ONE JSON document on stdout:
  {"images": [<file names>], "invoice": <name>, "invoiceText": <text>,
   "exact": [<original>, <copy>], "similar": [<original>, <jpeg>], "font": <path>}

- fixture-01.png .. fixture-50.png, 720 x 400. Every one but the invoice is 9 x 8 flat cells (80 x 50 px, aligned to
  dHash's 9 x 8 downscale) whose grey levels differ by >= 40 between horizontal neighbours, a per-image colour cast,
  plus per-pixel grain: a dHash that a re-encode does not move, and far (about 32 of 64 bits) from every other
  fixture (docs/lessons.md phase 3.2, "dHash fixtures").
- fixture-23.png is the invoice: `Hoá đơn 2026 INVOICE-7731` in black on white, 72 px Segoe UI (has the Vietnamese
  glyphs; the sidecar's OCR tests use the same font), so OCR has to read it. No file name carries the text: a search
  hit can only come from OCR.
- planted-exact.png: the bytes of fixture-07.png. planted-similar.jpg: fixture-09.png re-encoded as JPEG q85.
Deterministic: the same output on every run (seeded RNG).
"""

import json
import random
import shutil
import sys
import unicodedata
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

COUNT = 50
WIDTH, HEIGHT = 720, 400
COLS, ROWS = 9, 8  # dHash: 9 x 8 grey cells, compared horizontally
MIN_STEP = 40  # grey levels between horizontal neighbours
GRAIN = 6.0  # std dev of the per-pixel noise
CAST = 20  # max per-channel colour cast
INVOICE_INDEX = 23
INVOICE_TEXT = unicodedata.normalize("NFC", "Hoá đơn 2026 INVOICE-7731")
FONT = Path("C:/Windows/Fonts/segoeui.ttf")
FONT_SIZE = 72
EXACT_OF, SIMILAR_OF = 7, 9
JPEG_QUALITY = 85


def name(i: int) -> str:
    return f"fixture-{i:02d}.png"


def row_levels(rng: random.Random) -> list[int]:
    levels = [rng.randrange(256)]
    while len(levels) < COLS:
        options = [v for v in range(256) if abs(v - levels[-1]) >= MIN_STEP]
        levels.append(rng.choice(options))
    return levels


def cells_image(i: int) -> Image.Image:
    rng = random.Random(7919 * i)
    grey = np.zeros((HEIGHT, WIDTH), dtype=np.float64)
    cw, ch = WIDTH // COLS, HEIGHT // ROWS
    for r in range(ROWS):
        for c, level in enumerate(row_levels(rng)):
            grey[r * ch : (r + 1) * ch, c * cw : (c + 1) * cw] = level
    noise = np.random.default_rng(i).normal(0.0, GRAIN, size=(HEIGHT, WIDTH, 3))
    cast = np.array([rng.randint(-CAST, CAST) for _ in range(3)], dtype=np.float64)
    rgb = np.clip(grey[:, :, None] + cast[None, None, :] + noise, 0, 255).astype(np.uint8)
    return Image.fromarray(rgb, "RGB")


def invoice_image() -> Image.Image:
    if not FONT.is_file():
        raise FileNotFoundError(f"font with Vietnamese glyphs not found: {FONT}")
    font = ImageFont.truetype(str(FONT), FONT_SIZE)
    img = Image.new("RGB", (WIDTH * 2, HEIGHT), "white")
    draw = ImageDraw.Draw(img)
    left, top, right, bottom = draw.textbbox((0, 0), INVOICE_TEXT, font=font)
    x = (img.width - (right - left)) // 2 - left
    y = (img.height - (bottom - top)) // 2 - top
    draw.text((x, y), INVOICE_TEXT, fill="black", font=font)
    return img


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(argv[0])
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        print(f"out dir is not empty: {out}", file=sys.stderr)
        return 2
    images = []
    for i in range(1, COUNT + 1):
        img = invoice_image() if i == INVOICE_INDEX else cells_image(i)
        img.save(out / name(i), "PNG")
        images.append(name(i))
    shutil.copyfile(out / name(EXACT_OF), out / "planted-exact.png")
    with Image.open(out / name(SIMILAR_OF)) as src:
        src.convert("RGB").save(out / "planted-similar.jpg", "JPEG", quality=JPEG_QUALITY)
    images += ["planted-exact.png", "planted-similar.jpg"]
    doc = {
        "images": images,
        "invoice": name(INVOICE_INDEX),
        "invoiceText": INVOICE_TEXT,
        "exact": [name(EXACT_OF), "planted-exact.png"],
        "similar": [name(SIMILAR_OF), "planted-similar.jpg"],
        "font": str(FONT),
    }
    print(json.dumps(doc))  # ASCII escapes: stdout is a pipe in the locale's code page under -I
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
