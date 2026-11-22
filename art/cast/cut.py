# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow>=11"]
# ///
"""Cut Codex's puppet sheets (art/cast/raw/*-sheet.png) into parts.

Each sheet is a grid of separate pieces on a transparent background. The
pieces are found as connected regions of the alpha mask, named by the grid
cell their centre falls in, trimmed, cleaned of the generator's red fringe,
and scaled so every character's torso has the same height. Writes the parts
to src/assets/cast/<sheet>/<part>.png and their sizes to
src/scene/cast/parts.ts.

A sheet named <worker>-<build> drops its head, closed-eyes head and chair
unless the build is medium: the app takes those from the medium sheet.

Run: uv run --script art/cast/cut.py              (writes every sheet's parts)
     uv run --script art/cast/cut.py kit nell     (writes only these sheets' parts)
     uv run --script art/cast/cut.py --check      (writes nothing, exits 1 on a difference)
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / "art" / "cast" / "raw"
OUT = ROOT / "src" / "assets" / "cast"
MANIFEST = ROOT / "src" / "scene" / "cast" / "parts.ts"

# Grid layouts, row by row; None marks an empty cell.
PEOPLE: list[list[str | None]] = [
    ["head", "head-closed", "torso", "chair"],
    ["upper-arm-l", "upper-arm-r", "forearm-l", "forearm-r"],
    ["leg-l", "leg-r", "pelvis", None],
]
PETS: list[list[str | None]] = [
    ["cat-body", "cat-head", "cat-tail"],
    ["dog-body", "dog-head", "dog-tail"],
    ["owl-body", "owl-head", "shell"],
]
# Effort builds of a tier worker, lean to buff; non-medium builds take these from medium.
BUILDS = ("minimal", "low", "medium", "high", "xhigh", "max")
FROM_MEDIUM = ("head", "head-closed", "chair")
# Every person's torso is scaled to this height; the pets share one scale.
TORSO_PX = 96
PETS_SCALE = 0.25
# Labelling runs on a mask shrunk by this factor; specks smaller than MIN_AREA cells are fringe.
STEP = 4
MIN_AREA = 40
# Red fringe is cleaned only this close to a piece's edge.
EDGE_PX = 3


@dataclass
class Region:
    x0: int
    y0: int
    x1: int
    y1: int
    area: int

    def centre(self) -> tuple[float, float]:
        return ((self.x0 + self.x1) / 2, (self.y0 + self.y1) / 2)


def regions(img: Image.Image) -> list[Region]:
    """Connected opaque regions, in full-size pixel coordinates."""
    small = img.getchannel("A").resize((img.width // STEP, img.height // STEP))
    w, h = small.size
    data = small.tobytes()
    seen = bytearray(w * h)
    found: list[Region] = []
    for start in range(w * h):
        if seen[start] or data[start] < 32:
            continue
        seen[start] = 1
        stack = [start]
        r = Region(w, h, 0, 0, 0)
        while stack:
            i = stack.pop()
            x, y = i % w, i // w
            r.x0, r.y0, r.x1, r.y1 = min(r.x0, x), min(r.y0, y), max(r.x1, x), max(r.y1, y)
            r.area += 1
            for j in (i - 1 if x > 0 else -1, i + 1 if x < w - 1 else -1, i - w, i + w):
                if 0 <= j < w * h and not seen[j] and data[j] >= 32:
                    seen[j] = 1
                    stack.append(j)
        if r.area >= MIN_AREA:
            found.append(
                Region(r.x0 * STEP, r.y0 * STEP, (r.x1 + 1) * STEP, (r.y1 + 1) * STEP, r.area)
            )
    return found


def defringe(part: Image.Image) -> Image.Image:
    """Drop the saturated red speckle the generator leaves along cut edges.

    Only near an edge: red or orange clothing inside a piece stays.
    """
    rgba = part.convert("RGBA")
    data = bytearray(rgba.tobytes())
    edge = rgba.getchannel("A").filter(ImageFilter.MinFilter(EDGE_PX * 2 + 1)).tobytes()
    for i in range(0, len(data), 4):
        if edge[i // 4] < 32 and data[i + 3] and data[i] > 150 and data[i + 1] < 90 and data[i + 2] < 90:
            data[i + 3] = 0
    return Image.frombytes("RGBA", rgba.size, bytes(data))


def cut(sheet: Path, layout: list[list[str | None]]) -> dict[str, Image.Image]:
    img = Image.open(sheet).convert("RGBA")
    cols, rows = len(layout[0]), len(layout)
    parts: dict[str, Region] = {}
    for r in regions(img):
        cx, cy = r.centre()
        name = layout[min(int(cy * rows / img.height), rows - 1)][
            min(int(cx * cols / img.width), cols - 1)
        ]
        if name is None:
            continue
        old = parts.get(name)
        parts[name] = (
            r
            if old is None
            else Region(
                min(old.x0, r.x0), min(old.y0, r.y0), max(old.x1, r.x1), max(old.y1, r.y1),
                old.area + r.area,
            )
        )
    missing = [n for row in layout for n in row if n is not None and n not in parts]
    if missing:
        raise SystemExit(f"{sheet.name}: no piece found for {', '.join(missing)}")
    out: dict[str, Image.Image] = {}
    for name, r in parts.items():
        piece = defringe(img.crop((r.x0, r.y0, r.x1, r.y1)))
        box = piece.getbbox()
        out[name] = piece.crop(box) if box else piece
    return out


def scaled(parts: dict[str, Image.Image], factor: float) -> dict[str, Image.Image]:
    return {
        n: p.resize((max(1, round(p.width * factor)), max(1, round(p.height * factor))), Image.Resampling.LANCZOS)
        for n, p in parts.items()
    }


def build_of(cast: str) -> tuple[str, str] | None:
    worker, _, build = cast.rpartition("-")
    return (worker, build) if worker and build in BUILDS else None


def main() -> None:
    parser = argparse.ArgumentParser(description="Cut the puppet sheets into parts.")
    parser.add_argument("names", nargs="*", help="write parts only for these sheets or workers")
    parser.add_argument("--check", action="store_true", help="write nothing, exit 1 on a difference")
    args = parser.parse_args()
    names: list[str] = args.names
    check: bool = args.check

    sizes: dict[str, dict[str, tuple[int, int]]] = {}
    tiers: dict[str, list[str]] = {}
    for sheet in sorted(RAW.glob("*-sheet.png")):
        cast = sheet.name.removesuffix("-sheet.png")
        if cast == "pets":
            parts = scaled(cut(sheet, PETS), PETS_SCALE)
        else:
            raw = cut(sheet, PEOPLE)
            parts = scaled(raw, TORSO_PX / raw["torso"].height)
        tier = build_of(cast)
        if tier is not None:
            tiers.setdefault(tier[0], []).append(tier[1])
            if tier[1] != "medium":
                parts = {n: p for n, p in parts.items() if n not in FROM_MEDIUM}
        sizes[cast] = {n: p.size for n, p in sorted(parts.items())}
        if check or (names and not any(cast == n or cast.startswith(f"{n}-") for n in names)):
            continue
        folder = OUT / cast
        folder.mkdir(parents=True, exist_ok=True)
        for name, part in parts.items():
            part.save(folder / f"{name}.png", optimize=True)
        print(f"{cast}: {len(parts)} parts")

    lines = [
        "// Generated by art/cast/cut.py from Codex's puppet sheets. Do not edit.",
        "",
        "export interface PartSize {",
        "  readonly w: number;",
        "  readonly h: number;",
        "}",
        "",
    ]
    for cast, cast_sizes in sizes.items():
        ident = cast.replace("-", "_").upper()
        lines.append(f"export const {ident} = {{")
        for name, (w, h) in cast_sizes.items():
            lines.append(
                f'  "{name}": {{ src: new URL("../../assets/cast/{cast}/{name}.png", import.meta.url).href, w: {w}, h: {h} }},'
            )
        lines.append("} as const;")
        lines.append("")
    if tiers:
        lines.append("export const TIER_BUILDS = {")
        for worker in sorted(tiers):
            ordered = sorted(tiers[worker], key=BUILDS.index)
            entries = ", ".join(f'"{b}": {worker.upper()}_{b.upper()}' for b in ordered)
            lines.append(f'  "{worker}": {{ {entries} }},')
        lines.append("} as const;")
        lines.append("")
    text = "\n".join(lines)
    if check:
        raise SystemExit(difference(text, sizes))
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(text)


def difference(text: str, sizes: dict[str, dict[str, tuple[int, int]]]) -> str | None:
    """The first way the committed manifest and parts differ from a fresh cut, or None."""
    if not MANIFEST.exists() or MANIFEST.read_text() != text:
        return f"{MANIFEST.relative_to(ROOT)} differs from a fresh cut"
    for cast, cast_sizes in sizes.items():
        for name, size in cast_sizes.items():
            path = OUT / cast / f"{name}.png"
            if not path.exists():
                return f"{path.relative_to(ROOT)} is missing"
            with Image.open(path) as img:
                if img.size != size:
                    return f"{path.relative_to(ROOT)} is {img.size}, the cut gives {size}"
        tier = build_of(cast)
        if tier is not None and tier[1] != "medium":
            for name in FROM_MEDIUM:
                if (OUT / cast / f"{name}.png").exists():
                    return f"{cast}/{name}.png should not exist: it comes from the medium sheet"
    return None


if __name__ == "__main__":
    main()
