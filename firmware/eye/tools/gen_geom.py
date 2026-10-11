#!/usr/bin/env python3
"""Generate src/led/geom.rs from src/led/map.rs and the grid constants in src/led/grid.rs.
Rerun if either changes.

    python tools/gen_geom.py
    python tools/gen_geom.py --check   # fails if geom.rs is out of date
"""

import math
import re
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAP = ROOT / "src" / "led" / "map.rs"
GRID = ROOT / "src" / "led" / "grid.rs"
OUT = ROOT / "src" / "led" / "geom.rs"

PER_LINE = 6
IDX_PER_LINE = 16
PAIRS_PER_LINE = 3


def const(src: str, name: str, ty: str = "f32") -> float:
    m = re.search(rf"const {name}: {ty}\s*=\s*([-0-9.]+)\s*;", src)
    if not m:
        raise SystemExit(f"could not find constant {name}")
    return float(m.group(1))


def as_f32(v: float) -> float:
    """Round a Python float to the nearest f32, the value Rust will hold."""
    return struct.unpack("<f", struct.pack("<f", v))[0]


def dist_f32(dx: float, dy: float) -> float:
    """sqrt(dx^2 + dy^2), rounded to f32 at each step as the chip would."""
    d2 = as_f32(as_f32(dx * dx) + as_f32(dy * dy))
    return as_f32(math.sqrt(d2))


def lit(v: float) -> str:
    """Shortest Rust f32 literal that parses back to exactly this value."""
    target = as_f32(v)
    s = repr(target)
    for digits in range(1, 10):
        candidate = f"{target:.{digits}g}"
        if as_f32(float(candidate)) == target:
            s = candidate
            break
    if not any(c in s for c in ".eE"):
        s += ".0"
    return s


def emit(name: str, ty: str, doc: str, values: list[str], per_line: int) -> list[str]:
    lines = [f"/// {doc}", f"pub static {name}: {ty} = ["]
    for start in range(0, len(values), per_line):
        lines.append("    " + ", ".join(values[start : start + per_line]) + ",")
    lines += ["];", ""]
    return lines


def pair(a: float, b: float) -> str:
    return f"({lit(a)}, {lit(b)})"


def buckets(leds, cell_mm: float, cols: int, rows: int):
    """Group LED indices by grid cell, the way grid.rs looks them up."""
    cells: list[list[int]] = [[] for _ in range(cols * rows)]
    for i, (wx, wy, _, _) in enumerate(leds):
        cx = min(int(as_f32(wx / cell_mm)), cols - 1)
        cy = min(int(as_f32(wy / cell_mm)), rows - 1)
        cells[cy * cols + cx].append(i)

    start, flat = [0], []
    for cell in cells:
        flat.extend(cell)
        start.append(len(flat))
    return start, flat


def boards(leds, board_count: int):
    """Per-board center and (lowest, highest) wy, indexed by board id with index 0 unused."""
    sx = [0.0] * (board_count + 1)
    sy = [0.0] * (board_count + 1)
    n = [0] * (board_count + 1)
    y_range = [(0.0, 0.0)] + [(math.inf, -math.inf)] * board_count
    for wx, wy, board, _ in leds:
        sx[board] = as_f32(sx[board] + wx)
        sy[board] = as_f32(sy[board] + wy)
        n[board] += 1
        lo, hi = y_range[board]
        y_range[board] = (min(lo, wy), max(hi, wy))
    center = [(0.0, 0.0)] + [
        (as_f32(sx[b] / n[b]), as_f32(sy[b] / n[b])) for b in range(1, board_count + 1)
    ]
    return center, y_range


def main() -> None:
    src = MAP.read_text(encoding="utf-8")
    grid_src = GRID.read_text(encoding="utf-8")

    cell_mm = const(grid_src, "CELL_MM")
    cols = int(const(grid_src, "COLS", "usize"))
    rows = int(const(grid_src, "ROWS", "usize"))

    found = re.findall(
        r"Led\s*\{\s*wx:\s*([-0-9.]+),\s*wy:\s*([-0-9.]+),\s*board_id:\s*(\d+),\s*local_idx:\s*(\d+)",
        src,
    )
    if not found:
        raise SystemExit(f"no LED entries found in {MAP}")
    leds = [(as_f32(float(x)), as_f32(float(y)), int(b), int(l)) for x, y, b, l in found]
    board_count = max(b for _, _, b, _ in leds)
    leds_per_board = max(l for _, _, _, l in leds)
    if board_count * leds_per_board != len(leds):
        raise SystemExit(f"{len(leds)} LEDs is not {board_count} boards x {leds_per_board}")

    # The fixture's bounds: the triangle through the outer LED rows.
    left, right = min(l[0] for l in leds), max(l[0] for l in leds)
    top, bot = min(l[1] for l in leds), max(l[1] for l in leds)
    cx = as_f32((left + right) / 2.0)
    half_w = as_f32((right - left) / 2.0)
    height = as_f32(bot - top)
    centroid_x = cx
    centroid_y = as_f32(top + as_f32(height / 3.0))

    dist_c, theta_c, dist_apex = [], [], []
    for wx, wy, _, _ in leds:
        dx, dy = as_f32(wx - centroid_x), as_f32(wy - centroid_y)
        dist_c.append(dist_f32(dx, dy))
        theta_c.append(math.atan2(dy, dx))

        ax, ay = as_f32(wx - cx), as_f32(wy - bot)
        dist_apex.append(dist_f32(ax, ay))

    cell_start, cell_leds = buckets(leds, cell_mm, cols, rows)
    center, y_range = boards(leds, board_count)

    # Tiles that share an edge have the closest centers. The lattice of tile corners, with
    # the gaps between tiles included, follows from that spacing.
    spacing = min(
        math.dist(center[a], center[b])
        for a in range(1, board_count + 1)
        for b in range(a + 1, board_count + 1)
    )
    lattice_row_h = as_f32(spacing * 1.5)
    lattice_side = as_f32(spacing * math.sqrt(3.0))
    lattice_top_y = as_f32(min(c[1] for c in center[1:]) - lattice_row_h / 3.0)
    per_board = "[(f32, f32); BOARD_COUNT + 1]"

    body = "\n".join(
        [
            "// Auto-generated by tools/gen_geom.py from map.rs - do not edit by hand.",
            "// Per-LED and per-board geometry and the spatial grid buckets.",
            "",
            "// A table value can be close to a named constant such as pi/6 by chance.",
            "#![allow(clippy::approx_constant)]",
            "",
            "use super::grid::{COLS, ROWS};",
            "use super::map::LED_COUNT;",
            "",
            "/// Boards on the fixture, numbered from 1, and LEDs on each, numbered from 1.",
            f"pub const BOARD_COUNT: usize = {board_count};",
            f"pub const LEDS_PER_BOARD: usize = {leds_per_board};",
            "",
            "/// The fixture's bounds in mm: the triangle through its outer LED rows, point down.",
            f"pub const WORLD_LEFT: f32 = {lit(left)};",
            f"pub const WORLD_RIGHT: f32 = {lit(right)};",
            f"pub const WORLD_TOP: f32 = {lit(top)};",
            f"pub const WORLD_BOT: f32 = {lit(bot)};",
            f"pub const WORLD_CX: f32 = {lit(cx)};",
            f"pub const WORLD_HALF_W: f32 = {lit(half_w)};",
            f"pub const WORLD_H: f32 = {lit(height)};",
            "/// The triangle's centroid, a third of the way down from the top edge.",
            f"pub const WORLD_CENTROID_X: f32 = {lit(centroid_x)};",
            f"pub const WORLD_CENTROID_Y: f32 = {lit(centroid_y)};",
            "",
            "/// Distance in mm between the centers of two tiles that share an edge.",
            f"pub const TILE_SPACING_MM: f32 = {lit(spacing)};",
            "/// The lattice of tile corners, gaps included: the top row's y, the row height and",
            "/// the side length, in mm. Row r holds 6 - r corners centered on WORLD_CX.",
            f"pub const LATTICE_TOP_Y: f32 = {lit(lattice_top_y)};",
            f"pub const LATTICE_ROW_H: f32 = {lit(lattice_row_h)};",
            f"pub const LATTICE_SIDE: f32 = {lit(lattice_side)};",
            "",
            "/// Farthest LED from the fixture centroid, in mm.",
            f"pub const DIST_C_MAX: f32 = {lit(max(dist_c))};",
            "",
            *emit("DIST_C", "[f32; LED_COUNT]", "Distance in mm from the fixture centroid.",
                  [lit(v) for v in dist_c], PER_LINE),
            *emit("THETA_C", "[f32; LED_COUNT]", "Angle in radians from the fixture centroid.",
                  [lit(v) for v in theta_c], PER_LINE),
            *emit("DIST_APEX", "[f32; LED_COUNT]", "Distance in mm from the apex, the triangle's low point.",
                  [lit(v) for v in dist_apex], PER_LINE),
            *emit("BOARD_CENTER", per_board, "Each board's center (mean LED position), by board id. Index 0 is unused.",
                  [pair(*c) for c in center], PAIRS_PER_LINE),
            *emit("BOARD_Y_RANGE", per_board, "Each board's lowest and highest LED wy, by board id. Index 0 is unused.",
                  [pair(*r) for r in y_range], PAIRS_PER_LINE),
            *emit("CELL_START", "[u16; COLS * ROWS + 1]", "Where each grid cell's run begins in CELL_LEDS.",
                  [str(v) for v in cell_start], IDX_PER_LINE),
            *emit("CELL_LEDS", "[u16; LED_COUNT]", "LED indices grouped by grid cell, ascending within each cell.",
                  [str(v) for v in cell_leds], IDX_PER_LINE),
        ]
    )
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != body:
            raise SystemExit(f"{OUT} is out of date: run python tools/gen_geom.py")
        print(f"{OUT} is up to date")
        return
    OUT.write_text(body, encoding="utf-8")
    print(f"Wrote {OUT} ({len(leds)} LEDs, {board_count} boards, {cols}x{rows} grid)")


if __name__ == "__main__":
    main()
