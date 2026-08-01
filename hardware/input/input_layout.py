"""Draw the Triangel input board outline and place its footprints.

Run with KiCad's bundled Python, with the board CLOSED in the PCB editor:

    "C:\\Program Files\\KiCad\\10.0\\bin\\python.exe" input_layout.py

Safe to re-run: it replaces the board outline and repositions footprints, and leaves
tracks, zones and anything else alone. Import footprints first with
Tools > Update PCB from Schematic.

Coordinates below are board-local millimetres - (0, 0) is the top-left corner of the
board, X right, Y down - so the numbers read the same way the panel does.
"""

import math
import os

import pcbnew

BOARD_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "input.kicad_pcb")

# Board outline. The d-pad at a 19mm pitch needs 44mm; the rest is the slide switch,
# the IR sensor and margins.
WIDTH_MM   = 80.0
HEIGHT_MM  = 60.0
CORNER_R   = 3.0
EDGE_WIDTH = 0.1

# Top-left of the board on the drawing sheet.
ORIGIN_X   = 100.0
ORIGIN_Y   = 80.0

# Centre-to-centre spacing of the d-pad buttons. 19mm is standard keyboard key pitch.
BTN_PITCH  = 19.0
# Horizontal room is bounded by the slide switch on the right; vertical room by the USB-C
# body above and the board edge below, which leaves the cluster centre only ~2mm of play.
DPAD_X     = 31.0
DPAD_Y     = 33.0

# Distance from the USB-C footprint origin to its opening face, measured from the body
# outline in the footprint. The part is rotated 180 degrees so the opening points at the
# top edge of the board, which puts the origin this far below that edge.
USB_OPENING_OFFSET = 5.09
# Positive values move the socket inward from the board edge.
USB_INSET = 0.0

FRONT, BACK = "front", "back"

# ref: (x, y, rotation degrees, side)
PLACEMENTS = {
    # D-pad, in a plus. Up/down/left/right sit one pitch from the centre button.
    "SW1": (DPAD_X, DPAD_Y - BTN_PITCH, 0, FRONT),   # up
    "SW2": (DPAD_X, DPAD_Y + BTN_PITCH, 0, FRONT),   # down
    "SW3": (DPAD_X - BTN_PITCH, DPAD_Y, 0, FRONT),   # left
    "SW4": (DPAD_X + BTN_PITCH, DPAD_Y, 0, FRONT),   # right
    "SW5": (DPAD_X, DPAD_Y, 0, FRONT),               # centre

    # Sound switch and IR sensor share the right-hand strip.
    "SW6":  (64.0, 20.0, 0, FRONT),
    "CGQ1": (64.0, 46.0, 0, FRONT),

    # IR sensor's supply filter, on the back directly behind the sensor. The front then
    # carries only the parts that have to face the user.
    "R1": (58.0, 42.0, 90, BACK),
    "C1": (58.0, 46.0, 90, BACK),
    "C2": (58.0, 50.0, 90, BACK),

    # Back side: connector at top centre, expander behind the d-pad.
    # Rotation 0, not 180: Flip() below mirrors top-to-bottom, which already turns the
    # opening towards the top edge. Verified by checking that the connector's tail pads
    # end up on the inward side of its body.
    "USBC1": (WIDTH_MM / 2.0, USB_OPENING_OFFSET + USB_INSET, 0, BACK),
    "C3":    (WIDTH_MM / 2.0, 12.0, 0, BACK),
    "U1":    (40.0, 35.0, 0, BACK),
    "C4":    (40.0, 42.0, 0, BACK),

    # Mounting holes, inset from each corner.
    "H1": (5.0, 5.0, 0, FRONT),
    "H2": (WIDTH_MM - 5.0, 5.0, 0, FRONT),
    "H3": (5.0, HEIGHT_MM - 5.0, 0, FRONT),
    "H4": (WIDTH_MM - 5.0, HEIGHT_MM - 5.0, 0, FRONT),
}


def mm(value):
    return pcbnew.FromMM(value)


def pt(x, y):
    """Board-local millimetres to a point on the drawing sheet."""
    return pcbnew.VECTOR2I(mm(ORIGIN_X + x), mm(ORIGIN_Y + y))


def add_shape(board, configure):
    shape = pcbnew.PCB_SHAPE(board)
    configure(shape)
    shape.SetLayer(pcbnew.Edge_Cuts)
    shape.SetWidth(mm(EDGE_WIDTH))
    board.Add(shape)


def add_segment(board, start, end):
    def configure(shape):
        shape.SetShape(pcbnew.SHAPE_T_SEGMENT)
        shape.SetStart(start)
        shape.SetEnd(end)

    add_shape(board, configure)


def add_arc(board, start, mid, end):
    """Arc through three points - a midpoint avoids any ambiguity about direction."""

    def configure(shape):
        shape.SetShape(pcbnew.SHAPE_T_ARC)
        shape.SetArcGeometry(start, mid, end)

    add_shape(board, configure)


def draw_outline(board):
    # RemoveNative, not Remove: Remove leaves the shape orphaned and the drawing list
    # corrupt, which crashes the save.
    for shape in [d for d in board.GetDrawings() if d.GetLayer() == pcbnew.Edge_Cuts]:
        board.RemoveNative(shape)

    w, h, r = WIDTH_MM, HEIGHT_MM, CORNER_R
    # How far the middle of a corner arc sits in from the corner itself.
    k = r * (1.0 - math.sqrt(0.5))

    add_segment(board, pt(r, 0), pt(w - r, 0))      # top
    add_segment(board, pt(w, r), pt(w, h - r))      # right
    add_segment(board, pt(w - r, h), pt(r, h))      # bottom
    add_segment(board, pt(0, h - r), pt(0, r))      # left

    add_arc(board, pt(0, r), pt(k, k), pt(r, 0))
    add_arc(board, pt(w - r, 0), pt(w - k, k), pt(w, r))
    add_arc(board, pt(w, h - r), pt(w - k, h - k), pt(w - r, h))
    add_arc(board, pt(r, h), pt(k, h - k), pt(0, h - r))


def place_footprints(board):
    # Look footprints up through GetFootprints(): FindFootprintByReference returns an
    # untyped object whose methods are not bound.
    by_ref = {fp.GetReference(): fp for fp in board.GetFootprints()}
    missing = []
    for ref, (x, y, rotation, side) in sorted(PLACEMENTS.items()):
        fp = by_ref.get(ref)
        if fp is None:
            missing.append(ref)
            continue
        if (side == BACK) != fp.IsFlipped():
            # Mirror left-to-right so the part keeps its up/down geometry, which is what
            # the USB-C's opening direction depends on.
            fp.Flip(fp.GetPosition(), True)
        fp.SetOrientationDegrees(rotation)
        fp.SetPosition(pt(x, y))
    return missing


def main():
    board = pcbnew.LoadBoard(BOARD_PATH)
    draw_outline(board)
    missing = place_footprints(board)
    pcbnew.SaveBoard(BOARD_PATH, board)

    print("board %.0f x %.0f mm, %.0fmm corners" % (WIDTH_MM, HEIGHT_MM, CORNER_R))
    print("placed %d footprints" % (len(PLACEMENTS) - len(missing)))
    if missing:
        print("NOT FOUND: " + ", ".join(missing))


if __name__ == "__main__":
    main()
