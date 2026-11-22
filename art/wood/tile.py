# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow>=11"]
# ///
"""Shrink the desk wood swatches (art/wood/raw/<wood>.png) to the tiles the app ships.

Writes src/assets/wood/<wood>.jpg, 192 by 192, and prints each tile's size and
the raw swatch's mean colour, which is the wood's flat colour in the app.

Run: uv run --script art/wood/tile.py
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / "art" / "wood" / "raw"
OUT = ROOT / "src" / "assets" / "wood"
TILE_PX = 192


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for swatch in sorted(RAW.glob("*.png")):
        with Image.open(swatch) as raw:
            img = raw.convert("RGB")
        tile = OUT / f"{swatch.stem}.jpg"
        img.resize((TILE_PX, TILE_PX), Image.Resampling.LANCZOS).save(tile, quality=80, optimize=True)
        mean = img.resize((1, 1), Image.Resampling.BOX).tobytes().hex()
        print(f"{swatch.stem}: {tile.stat().st_size} bytes, mean #{mean}")


if __name__ == "__main__":
    main()
