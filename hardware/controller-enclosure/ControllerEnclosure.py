# ControllerEnclosure.py
# Fusion 360 script: back tray and front plate for the Triangel controller board.
#
# Run: open a new, empty design, then Utilities > Add-Ins > Scripts and Add-Ins, pick this
# script and Run. It builds two bodies, ControllerEnclosure_BackTray and
# ControllerEnclosure_FrontPlate, and shows a summary when it is done.
#
# Fit check: export the board from KiCad as STEP (origin "User defined" at 0, 0, with
# "Substitute similarly named models" ticked) and insert it into the same design without
# moving it. It sits in place when ALIGN_TO_STEP is True.
#
# Assembly: an M3x4x3 heat-set insert in each of the four bosses, and four M3x10 countersunk
# screws through the plate and the board.
#
# Units are mm. Board coordinates as in controller.kicad_pcb: origin at the board's top-left,
# X right, Y down. Z = 0 is the board's back face, +Z toward the room.

import adsk.core, adsk.fusion, math, traceback

# ---- Board: values from controller.kicad_pcb; change them only if the board changes ----
BOARD_W, BOARD_H = 70.0, 46.0
BOARD_R          = 3.0     # board corner radius
BOARD_T          = 1.6

HOLES    = [(5.0, 5.0), (65.0, 5.0), (5.0, 41.0), (65.0, 41.0)]   # M3 mounting holes
# SW1 up, SW3 down, SW5 left, SW4 right, SW6 center
BUTTONS  = [(35.0, 10.0), (35.0, 36.0), (22.0, 23.0), (48.0, 23.0), (35.0, 23.0)]
SWITCH   = (12.35, 15.0)   # SW2 slide switch
SENSOR   = (57.0, 35.0)    # CGQ1 IR sensor
MIC_PORT = (44.6, 10.0)    # MK1's sound hole

# Front parts.
BTN_TOP   = [(2.9, -1.25), (2.9, 1.25), (1.5, 3.06), (-1.5, 3.06),     # button housing outline,
             (-2.9, 1.25), (-2.9, -1.25), (-1.5, -3.06), (1.5, -3.06)]  # from 2.2 to 3.5 tall
SW_BODY   = (12.0, 3.5)          # slide switch body
IR_BODY   = (4.0, 5.0)           # IR sensor body
IR_BODY_H = 2.7                  # its height
IR_DOME   = (0.0, -0.81)         # its lens dome's center, offset from the sensor's center
BTN_TOP_Z, SW_TOP_Z, IR_TOP_Z = 5.0, 5.5, 4.2   # height of each part above the board

# Back connectors with cables through the walls.
TILE_USB_X = [17.0, 53.0]  # USBC1 (tiles 1-12) and USBC2 (tiles 13-25), top edge
USB_AXIS   = 1.63          # center of their openings, below the board's back
JACK_X     = 48.0          # 12V jack U3, bottom edge: center of its body
JACK_W     = 9.0           # its body width
JACK_H     = 11.0          # how far it hangs below the board's back

# Front copper the plate must clear, as (x0, y0, x1, y1).
JACK_PADS    = (42.5, 33.8, 49.8, 41.6)    # the jack's three pads
JACK_PIN_OUT = 2.4                         # how far its pins stand out of them

# ---- Settings (mm) --------------------------------------------------------------------
FIT        = 0.5    # clearance around the board inside the tray
WALL       = 2.0    # tray wall and floor thickness
FRONT_T    = 4.2    # front plate thickness; sets how far the parts' tops stand above it

SKIN       = 1.5    # plate thickness over the hollow underside; at least SKIN_MIN
CAVITY_INSET = 0.5  # hollow's edge, in from the board's edge
CORNER_BLOCK = 9.5  # solid square under each screw hole, from the board's corner
CORNER_CHAMFER = 2.0  # cut off the inner corner of each solid square
MIC_BOSS_D = 4.0    # solid column around the mic hole, sealing it against the board

BODY_CLR   = 0.4    # clearance around each front part's body in its hole
TOP_CLR    = 0.3    # clearance over the IR sensor's body
HOLE_R     = 0.6    # corner radius of the holes
RIM_SLOPE  = 0.5    # 45-degree rim where a hole meets the face
IR_WINDOW_D = 3.6   # IR lens window where it meets the pocket; it widens at 45 degrees to the face
MIC_HOLE_D = 1.5    # mic hole, at least the board's 0.5

PIN_CLR    = 0.4    # pocket clearance around the jack's pins
SKIN_MIN   = 1.2    # thinnest plastic left over any pocket
POCKET_R   = 1.0    # corner radius of the jack's pin pocket

GRIP_W      = 12.35  # largest USB-C plug grip, width
GRIP_T      = 6.5    # and thickness
GRIP_CLR    = 0.5    # room under the grip in the tile slots
JACK_CLR    = 0.3    # clearance around the jack's body in its slot
TAB_CLR     = 0.15   # clearance around the plate's tab above the jack

BOSS_D     = 9.6    # boss size; 9.6 max, parts on the board's back sit close to the bosses
INSERT_D   = 3.6    # insert pocket diameter (M3x4x3)
INSERT_L   = 5.0    # insert pocket depth
MOUTH_R    = 0.5    # round at the pocket mouth
SCREW_CLR  = 3.4    # screw hole through the plate
SCREW_HEAD = 6.5    # countersink diameter
CSK_DEPTH  = 1.8    # countersink depth

KEYHOLES   = [(13.0, 24.0), (57.0, 24.0)]   # keyhole centers
KEY_BIG_D  = 8.5    # round end, fits over the screw head
KEY_SLOT_D = 4.5    # slot width, fits the screw shank
KEY_RISE   = 8.0    # slot length, toward the top
LOCK_HOLE  = (35.0, 10.0)                   # locking screw; keep it off the keyholes' row
LOCK_D     = 4.5    # its hole
LOCK_REC_D = 12.0   # head recess on the inside of the floor, diameter
LOCK_REC_H = 0.8    # and depth

ENGRAVE_D   = 0.5            # engraving depth
TOP_LABELS  = [('1-12', 17.0), ('13-25', 53.0)]   # text and center X on the tray's top face
BOTTOM_LABEL = (['12 V', 'ONLY'], 34.0)   # lines of text and center X on the tray's bottom face
BOTTOM_LABEL_PITCH = 5.0     # distance between its lines' centers
TRAY_LABEL_H = 3.5           # letter height of the labels on the tray
TRAY_LABEL_LINE = 0.6        # and line width
FRONT_LABEL_H = 1.8          # letter height on the front
FRONT_LABEL_LINE = 0.5       # and line width; 0.5 is the finest that engraves cleanly
HOLD_AT     = (35.0, 16.5)   # center of "HOLD", above the center button
AUTO_AT     = (12.35, 9.9)   # center of "AUTO", above the switch
ARROWS      = [((35.0, 3.0), 'up'), ((35.0, 43.0), 'down'),
               ((15.2, 23.0), 'left'), ((54.8, 23.0), 'right')]   # triangle centers
ARROW_W, ARROW_H = 3.2, 2.8  # triangle base and height
NOTE_AT     = (2.3, 15.0)    # music note, left of the switch (sound on)
NOTE_OFF_AT = (22.8, 15.0)   # note in a slashed ring, right of the switch (sound off)
NOTE_H      = 3.6            # music note height
RING_D, RING_W = 4.6, 0.5    # slashed ring: outer diameter and line width
RING_NOTE   = 0.7            # size of the note inside the ring, against NOTE_H

EDGE_R_TRAY  = 1.8  # rounding on the tray's edges; under WALL
EDGE_R_CAV   = 1.5  # rounding where the inside walls meet the floor
EDGE_R_PLATE = 2.5  # rounding on the plate's top edge
NOTCH_R      = 1.0  # rounding along the tile slots' outside edges
SLOT_CORNER_R = 1.5  # rounding in the bottom corners of the tile slots

EXPLODE       = 0.0   # lifts the plate off for viewing; 0 for the real assembly
ALIGN_TO_STEP = True  # True: lines up with KiCad's STEP export; False: board top-left at 0, 0
# ------------------------------------------------------------------------------------

MM = 0.1   # Fusion works in cm

OUTER_W = BOARD_W + 2 * (FIT + WALL)
OUTER_H = BOARD_H + 2 * (FIT + WALL)
OUTER_R_XY = BOARD_R + FIT + WALL
INNER_R_XY = BOARD_R + FIT

BACK_CLEAR  = JACK_H + JACK_CLR     # gap behind the board, set by the jack
Z_PCB_BACK  = 0.0
Z_PCB       = BOARD_T
Z_FLOOR_IN  = Z_PCB_BACK - BACK_CLEAR
Z_FLOOR_OUT = Z_FLOOR_IN - WALL
Z_PLATE_IN  = Z_PCB + EXPLODE
Z_PLATE_OUT = Z_PCB + FRONT_T + EXPLODE

Z_TILE_SLOT = Z_PCB_BACK - USB_AXIS - GRIP_T / 2.0 - GRIP_CLR       # tile slot bottom
TILE_SLOT_W = 2.0 * (TILE_USB_X[0] - HOLES[0][0] - BOSS_D / 2.0)    # flush with the bosses
JACK_SLOT_W = JACK_W + 2 * JACK_CLR

POCKET   = (JACK_PADS[0] - PIN_CLR, JACK_PADS[1] - PIN_CLR,
            JACK_PADS[2] + PIN_CLR, JACK_PADS[3] + PIN_CLR)
POCKET_D = JACK_PIN_OUT + PIN_CLR

# Board top-left in KiCad's STEP export is (100, -80).
OX = 100.0 if ALIGN_TO_STEP else 0.0
OY = -126.0 if ALIGN_TO_STEP else 0.0


def offset_poly(pts, d):
    """Grow a convex polygon outward by d."""
    n = len(pts)
    area = sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1] for i in range(n))
    s = 1.0 if area > 0 else -1.0
    lines = []
    for i in range(n):
        (x0, y0), (x1, y1) = pts[i], pts[(i + 1) % n]
        L = math.hypot(x1 - x0, y1 - y0)
        lines.append(((x0 + s * (y1 - y0) / L * d, y0 - s * (x1 - x0) / L * d), (x1 - x0, y1 - y0)))
    out = []
    for i in range(n):
        (p, u), (q, v) = lines[i - 1], lines[i]
        k = ((q[0] - p[0]) * v[1] - (q[1] - p[1]) * v[0]) / (u[0] * v[1] - u[1] * v[0])
        out.append((p[0] + u[0] * k, p[1] + u[1] * k))
    return out


def note_shapes(scale):
    """Music note around (0, 0), Y up: ('circle', center, radius) and ('poly', points)."""
    s = scale * NOTE_H / 3.6
    head = (-0.45 * s, -1.05 * s)
    r = 0.75 * s
    stem_x1 = head[0] + r
    stem_x0 = stem_x1 - max(0.5, 0.5 * s)
    top = 1.8 * s
    fl = max(0.55, 0.7 * s)                      # flag thickness
    return [('circle', head, r),
            ('poly', [(stem_x0, head[1]), (stem_x1, head[1]), (stem_x1, top), (stem_x0, top)]),
            ('poly', [(stem_x1, top), (stem_x1 + 1.0 * s, top - 0.85 * s),
                      (stem_x1 + 1.0 * s, top - 0.85 * s - fl), (stem_x1, top - fl)])]


def arrow_points(direction):
    """Solid triangle around (0, 0), Y up, pointing 'up', 'down', 'left' or 'right'."""
    w, h = ARROW_W / 2.0, ARROW_H / 2.0
    tri = {'up': [(0, h), (-w, -h), (w, -h)], 'down': [(0, -h), (w, h), (-w, h)],
           'left': [(-h, 0), (h, -w), (h, w)], 'right': [(h, 0), (-h, w), (-h, -w)]}
    return tri[direction]


def slash_points():
    """Bar across the ring, upper left to lower right, Y up."""
    a, half, t = math.radians(-45), RING_D / 2.0, RING_W / 2.0
    dx, dy = math.cos(a), math.sin(a)
    return [(-dx * half - dy * t, -dy * half + dx * t), (dx * half - dy * t, dy * half + dx * t),
            (dx * half + dy * t, dy * half - dx * t), (-dx * half + dy * t, -dy * half - dx * t)]


def arc(cx, cy, rx, ry, a0, a1, step=10.0):
    """Points along an elliptical arc from angle a0 to a1 (degrees)."""
    n = max(2, int(abs(a1 - a0) / step) + 1)
    return [(cx + rx * math.cos(math.radians(a0 + (a1 - a0) * k / (n - 1))),
             cy + ry * math.sin(math.radians(a0 + (a1 - a0) * k / (n - 1)))) for k in range(n)]


# Lettering: centerline strokes of each character, 1 tall, Y up: (width, [stroke, ...]).
GLYPHS = {
    '1': (0.0, [[(0.0, 0.0), (0.0, 1.0)]]),
    '-': (0.36, [[(0.0, 0.5), (0.36, 0.5)]]),
    '2': (0.56, [arc(0.28, 0.72, 0.28, 0.28, 160, -20) + [(0.0, 0.0), (0.56, 0.0)]]),
    '3': (0.54, [arc(0.27, 0.755, 0.26, 0.245, 150, -90),
                 arc(0.27, 0.255, 0.28, 0.255, 90, -150)]),
    '5': (0.56, [[(0.54, 1.0), (0.06, 1.0), (0.03, 0.56)] + arc(0.28, 0.31, 0.28, 0.31, 120, -150)]),
    'H': (0.6, [[(0.0, 0.0), (0.0, 1.0)], [(0.6, 0.0), (0.6, 1.0)], [(0.0, 0.5), (0.6, 0.5)]]),
    'O': (0.64, [arc(0.32, 0.5, 0.32, 0.5, 90, 450)]),
    'L': (0.5, [[(0.0, 1.0), (0.0, 0.0), (0.5, 0.0)]]),
    'D': (0.6, [[(0.3, 0.0), (0.0, 0.0), (0.0, 1.0), (0.3, 1.0)] + arc(0.3, 0.5, 0.3, 0.5, 90, -90)]),
    'A': (0.64, [[(0.0, 0.0), (0.32, 1.0), (0.64, 0.0)], [(0.12, 0.36), (0.52, 0.36)]]),
    'U': (0.6, [[(0.0, 1.0), (0.0, 0.3)] + arc(0.3, 0.3, 0.3, 0.3, 180, 360) + [(0.6, 1.0)]]),
    'T': (0.64, [[(0.0, 1.0), (0.64, 1.0)], [(0.32, 1.0), (0.32, 0.0)]]),
    'V': (0.64, [[(0.0, 1.0), (0.32, 0.0), (0.64, 1.0)]]),
    'N': (0.6, [[(0.0, 0.0), (0.0, 1.0), (0.6, 0.0), (0.6, 1.0)]]),
    'Y': (0.64, [[(0.0, 1.0), (0.32, 0.5), (0.64, 1.0)], [(0.32, 0.5), (0.32, 0.0)]]),
    ' ': (0.1, []),
}


def _offset(pts, d, closed):
    """A polyline moved sideways by d (left of travel), corners rounded on the outside."""
    n = len(pts)

    def normal(i):
        (x0, y0), (x1, y1) = pts[i], pts[i + 1]
        L = math.hypot(x1 - x0, y1 - y0)
        return (-(y1 - y0) / L, (x1 - x0) / L)

    out = []
    for i in range(n):
        if closed and i == n - 1:
            break
        if not closed and i in (0, n - 1):
            nx, ny = normal(0 if i == 0 else n - 2)
            out.append((pts[i][0] + nx * d, pts[i][1] + ny * d))
            continue
        (ax, ay), (bx, by) = normal(i - 1 if i > 0 else n - 2), normal(i if i < n - 1 else 0)
        p = pts[i]
        mx, my = ax + bx, ay + by
        ml = math.hypot(mx, my)
        if ((ax * by - ay * bx < 0) == (d > 0)) and ml / 2.0 < 0.97:
            a0, a1 = math.atan2(ay * d, ax * d), math.atan2(by * d, bx * d)
            da = (a1 - a0 + math.pi) % (2 * math.pi) - math.pi
            steps = max(2, int(abs(da) / math.radians(15)) + 1)
            out += [(p[0] + abs(d) * math.cos(a0 + da * j / steps),
                     p[1] + abs(d) * math.sin(a0 + da * j / steps)) for j in range(steps + 1)]
        else:
            k = d / (ml / 2.0 * ml)
            out.append((p[0] + mx * k, p[1] + my * k))
    return out


def stroke_loops(pts, half):
    """Outline of a line `half` wide each side: one loop with round ends for an open line, an
    outer and an inner loop for a closed one."""
    pts = [p for i, p in enumerate(pts) if i == 0 or math.dist(p, pts[i - 1]) > 1e-6]
    if math.dist(pts[0], pts[-1]) < 1e-6:
        return [_offset(pts, half, True), _offset(pts, -half, True)]
    left, right = _offset(pts, half, False), _offset(pts, -half, False)

    def cap(c, frm, steps=6):
        a0 = math.atan2(frm[1] - c[1], frm[0] - c[0])
        return [(c[0] + half * math.cos(a0 - math.pi * k / steps),
                 c[1] + half * math.sin(a0 - math.pi * k / steps)) for k in range(1, steps)]

    return [left + cap(pts[-1], left[-1]) + right[::-1] + cap(pts[0], right[0])]


def text_strokes(text, height, line, gap=0.22):
    """Outline loops of each stroke of `text`, letters `height` tall, centered on (0, 0), Y up.
    Returns (strokes, width)."""
    s = height - line
    x, placed = 0.0, []
    for ch in text:
        w, strokes = GLYPHS[ch]
        placed += [[(x + line / 2 + px * s, line / 2 + py * s) for px, py in st] for st in strokes]
        x += w * s + line + gap * height
    width = x - gap * height
    return [stroke_loops([(px - width / 2, py - height / 2) for px, py in st], line / 2)
            for st in placed], width


def run(context):
    ui = None
    try:
        app = adsk.core.Application.get()
        ui = app.userInterface
        design = adsk.fusion.Design.cast(app.activeProduct)
        if not design:
            ui.messageBox('Open a new, empty Fusion design first, then run this script.')
            return
        for what, depth in (('the hollow underside', FRONT_T - SKIN), ('the jack pins', POCKET_D),
                            ('the IR sensor', IR_BODY_H + TOP_CLR)):
            if FRONT_T < depth + SKIN_MIN - 1e-9:
                ui.messageBox('FRONT_T %.1f leaves %.1fmm over %s, under SKIN_MIN %.1f.\n'
                              'Nothing was built.' % (FRONT_T, FRONT_T - depth, what, SKIN_MIN))
                return

        root = design.rootComponent
        exts = root.features.extrudeFeatures

        def fy(y):
            """Board Y (down) -> model Y (up)."""
            return BOARD_H - y

        OFF = FIT + WALL

        def plane_at(z):
            if abs(z) < 1e-9:
                return root.xYConstructionPlane
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * MM))
            return root.constructionPlanes.add(pin)

        def P(x, y):
            return adsk.core.Point3D.create((x + OX) * MM, (y + OY) * MM, 0)

        def rect_sketch(z, x0, y0, x1, y1):
            sk = root.sketches.add(plane_at(z))
            sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0, y0), P(x1, y1))
            return sk

        def rounded_rect_sketch(z, cx, cy, w, h, r):
            """Rounded rectangle centered on (cx, cy), model coordinates."""
            sk = root.sketches.add(plane_at(z))
            lines, arcs = sk.sketchCurves.sketchLines, sk.sketchCurves.sketchArcs
            hw, hh = w / 2.0, h / 2.0
            k = r * (1.0 - math.sqrt(0.5))
            lines.addByTwoPoints(P(cx - hw + r, cy - hh), P(cx + hw - r, cy - hh))
            arcs.addByThreePoints(P(cx + hw - r, cy - hh),
                                  P(cx + hw - k, cy - hh + k), P(cx + hw, cy - hh + r))
            lines.addByTwoPoints(P(cx + hw, cy - hh + r), P(cx + hw, cy + hh - r))
            arcs.addByThreePoints(P(cx + hw, cy + hh - r),
                                  P(cx + hw - k, cy + hh - k), P(cx + hw - r, cy + hh))
            lines.addByTwoPoints(P(cx + hw - r, cy + hh), P(cx - hw + r, cy + hh))
            arcs.addByThreePoints(P(cx - hw + r, cy + hh),
                                  P(cx - hw + k, cy + hh - k), P(cx - hw, cy + hh - r))
            lines.addByTwoPoints(P(cx - hw, cy + hh - r), P(cx - hw, cy - hh + r))
            arcs.addByThreePoints(P(cx - hw, cy - hh + r),
                                  P(cx - hw + k, cy - hh + k), P(cx - hw + r, cy - hh))
            return sk

        def rounded_box_sketch(z, box, r):
            """Rounded rectangle from a board-coordinate box (x0, y0, x1, y1)."""
            x0, y0, x1, y1 = box
            return rounded_rect_sketch(z, (x0 + x1) / 2.0, fy((y0 + y1) / 2.0),
                                       x1 - x0, y1 - y0, r)

        def rounded_poly_sketch(z, cx, cy, pts, r):
            """Convex polygon with rounded corners. pts are offsets from (cx, cy) in board
            directions (Y down); (cx, cy) is in model coordinates."""
            q = [(cx + px, cy - py) for px, py in pts]
            n = len(q)
            corners = []
            for i in range(n):
                (ax, ay), (bx, by), (nx, ny) = q[i - 1], q[i], q[(i + 1) % n]
                la, ln = math.hypot(ax - bx, ay - by), math.hypot(nx - bx, ny - by)
                u1 = ((ax - bx) / la, (ay - by) / la)
                u2 = ((nx - bx) / ln, (ny - by) / ln)
                half = math.acos(max(-1.0, min(1.0, u1[0] * u2[0] + u1[1] * u2[1]))) / 2.0
                t = r / math.tan(half)
                bl = math.hypot(u1[0] + u2[0], u1[1] + u2[1])
                bis = ((u1[0] + u2[0]) / bl, (u1[1] + u2[1]) / bl)
                d = r / math.sin(half) - r
                corners.append(((bx + u1[0] * t, by + u1[1] * t),
                                (bx + bis[0] * d, by + bis[1] * d),
                                (bx + u2[0] * t, by + u2[1] * t)))
            sk = root.sketches.add(plane_at(z))
            lines, arcs = sk.sketchCurves.sketchLines, sk.sketchCurves.sketchArcs
            for i in range(n):
                t1, mid, t2 = corners[i]
                arcs.addByThreePoints(P(*t1), P(*mid), P(*t2))
                lines.addByTwoPoints(P(*t2), P(*corners[(i + 1) % n][0]))
            return sk

        def circle_sketch(z, cx, cy, dia):
            sk = root.sketches.add(plane_at(z))
            sk.sketchCurves.sketchCircles.addByCenterRadius(P(cx, cy), (dia / 2.0) * MM)
            return sk

        def poly_sketch(z, pts):
            """Closed polygon through `pts`, model coordinates."""
            sk = root.sketches.add(plane_at(z))
            lines = sk.sketchCurves.sketchLines
            for i in range(len(pts)):
                lines.addByTwoPoints(P(*pts[i]), P(*pts[(i + 1) % len(pts)]))
            return sk

        skipped = []

        def engrave(profile, depth, body, what, symmetric=False):
            """Cut `profile` (a profile or sketch text) `depth` into `body`; on failure, note
            `what` in the summary and carry on."""
            try:
                inp = exts.createInput(profile, adsk.fusion.FeatureOperations.CutFeatureOperation)
                if symmetric:
                    inp.setSymmetricExtent(adsk.core.ValueInput.createByReal(2 * depth * MM), True)
                else:
                    inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(-depth * MM))
                inp.participantBodies = [body]
                exts.add(inp)
            except:
                skipped.append(what)

        def engrave_strokes(strokes, sketch_plane, to_sketch, body, what, symmetric=False):
            """Engrave lettering strokes (from text_strokes); `to_sketch` turns a lettering
            point into a sketch point on `sketch_plane`."""
            for loops in strokes:
                try:
                    sk = root.sketches.add(sketch_plane)
                    lines = sk.sketchCurves.sketchLines
                    for loop in loops:
                        pts = [p for i, p in enumerate(loop) if math.dist(p, loop[i - 1]) > 1e-4]
                        for i in range(len(pts)):
                            lines.addByTwoPoints(to_sketch(*pts[i]), to_sketch(*pts[(i + 1) % len(pts)]))
                    profiles = adsk.core.ObjectCollection.create()
                    for i in range(sk.profiles.count):
                        pr = sk.profiles.item(i)
                        if len(loops) == 1 or pr.profileLoops.count == 2:
                            profiles.add(pr)
                    engrave(profiles, ENGRAVE_D, body, what, symmetric)
                except:
                    skipped.append(what)

        def front_text(text, bx, by, body):
            """Engrave `text` on the plate's face, centered on board point (bx, by)."""
            strokes, _ = text_strokes(text, FRONT_LABEL_H, FRONT_LABEL_LINE)
            my = fy(by)
            engrave_strokes(strokes, plane_at(Z_PLATE_OUT), lambda x, y: P(bx + x, my + y), body, text)

        def front_symbol(shapes, bx, by, body, what):
            """Engrave shapes from note_shapes / arrow_points (Y up) around board point (bx, by)."""
            my = fy(by)
            for kind, *geo in shapes:
                if kind == 'circle':
                    (cx, cy), r = geo
                    sk = circle_sketch(Z_PLATE_OUT, bx + cx, my + cy, 2 * r)
                else:
                    sk = poly_sketch(Z_PLATE_OUT, [(bx + x, my + y) for x, y in geo[0]])
                engrave(sk.profiles.item(0), ENGRAVE_D, body, what)

        def wall_text(text, x, z_mid, face_y, up, body):
            """Engrave `text` on the tray's top or bottom face (model Y `face_y`), centered at X
            `x` and height `z_mid`, reading from in front. `up` is the Z direction of the
            letters' tops: -1 (toward the back) on the top face, 1 on the bottom face."""
            try:
                target = (face_y + OY) * MM
                pl = None
                for sign in (1.0, -1.0):
                    pin = root.constructionPlanes.createInput()
                    pin.setByOffset(root.xZConstructionPlane,
                                    adsk.core.ValueInput.createByReal(sign * target))
                    pl = root.constructionPlanes.add(pin)
                    g = pl.geometry
                    if abs(g.origin.vectorTo(adsk.core.Point3D.create(0, target, 0))
                           .dotProduct(g.normal)) < 1e-6:
                        break
                    pl.deleteMe()
                probe = root.sketches.add(pl)
                strokes, _ = text_strokes(text, TRAY_LABEL_H, TRAY_LABEL_LINE)

                def to_sketch(gx, gy):
                    p = probe.modelToSketchSpace(adsk.core.Point3D.create(
                        (x + gx + OX) * MM, target, (z_mid + up * gy) * MM))
                    return adsk.core.Point3D.create(p.x, p.y, 0)

                engrave_strokes(strokes, pl, to_sketch, body, text, symmetric=True)
                probe.deleteMe()
            except:
                skipped.append(text)

        def solid(sk, z0, z1, body):
            """Extrude a profile from z0 to z1; a new body, or joined to `body`."""
            new = body is None
            o = (adsk.fusion.FeatureOperations.NewBodyFeatureOperation if new
                 else adsk.fusion.FeatureOperations.JoinFeatureOperation)
            inp = exts.createInput(sk.profiles.item(0), o)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal((z1 - z0) * MM))
            if not new:
                inp.participantBodies = [body]
            f = exts.add(inp)
            return f.bodies.item(0) if new else body

        def cut(sk, depth, body):
            """Cut a profile `depth` along Z (negative = down)."""
            inp = exts.createInput(sk.profiles.item(0),
                                   adsk.fusion.FeatureOperations.CutFeatureOperation)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(depth * MM))
            inp.participantBodies = [body]
            exts.add(inp)

        def loft_cut(top_sk, bottom_sk, body):
            """Cut the solid between two matching profiles."""
            li = root.features.loftFeatures.createInput(
                adsk.fusion.FeatureOperations.CutFeatureOperation)
            li.loftSections.add(top_sk.profiles.item(0))
            li.loftSections.add(bottom_sk.profiles.item(0))
            li.isSolid = True
            try:
                li.participantBodies = [body]
            except:
                pass
            root.features.loftFeatures.add(li)

        def round_vertical_edges(body, radius, corners):
            """Round the vertical edges at the given XY corners."""
            for r in [radius, radius * 0.75, radius * 0.5]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    g = e.geometry
                    if isinstance(g, adsk.core.Line3D):
                        a, b = g.startPoint, g.endPoint
                        if abs(a.x - b.x) < 1e-6 and abs(a.y - b.y) < 1e-6:
                            if any(abs(a.x - (cx + OX) * MM) < 0.05 * MM
                                   and abs(a.y - (cy + OY) * MM) < 0.05 * MM for cx, cy in corners):
                                edges.add(e)
                if not edges.count:
                    return
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(r * MM), True)
                try:
                    root.features.filletFeatures.add(fin)
                    return
                except:
                    continue

        def in_plane(edge, z):
            bb = edge.boundingBox
            return (abs(bb.minPoint.z - z * MM) < 0.02 * MM
                    and abs(bb.maxPoint.z - z * MM) < 0.02 * MM)

        def in_box(edge, box):
            x0, x1, y0, y1 = box
            bb = edge.boundingBox
            return (bb.minPoint.x > (x0 + OX) * MM and bb.maxPoint.x < (x1 + OX) * MM
                    and bb.minPoint.y > (y0 + OY) * MM and bb.maxPoint.y < (y1 + OY) * MM)

        def fillet_edges(body, radius, keep_flat=(), skip=()):
            """Round every edge except those in a `keep_flat` plane or a `skip` XY box.
            Falls back to smaller radii if a set fails; returns the radius used, 0.0 if none."""
            for r in [radius, radius * 0.75, radius * 0.5, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    if any(in_plane(e, z) for z in keep_flat):
                        continue
                    if any(in_box(e, box) for box in skip):
                        continue
                    edges.add(e)
                if not edges.count:
                    return 0.0
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(r * MM), True)
                try:
                    root.features.filletFeatures.add(fin)
                    return r
                except:
                    continue
            return 0.0

        def fillet_region(body, z, x0, x1, y0, y1, radius, skip=()):
            """Round the edges in plane `z` inside an XY box, minus `skip` boxes.
            Returns the radius used, 0.0 if none."""
            for r in [radius, radius * 0.6, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    if (in_plane(e, z) and in_box(e, (x0, x1, y0, y1))
                            and not any(in_box(e, box) for box in skip)):
                        edges.add(e)
                if not edges.count:
                    return 0.0
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(r * MM), True)
                try:
                    root.features.filletFeatures.add(fin)
                    return r
                except:
                    continue
            return 0.0

        def fillet_slot(body, x0, x1, face_y, radius):
            """Round a slot's edges on the outer face at model Y `face_y`.
            Returns the radius used, 0.0 if none."""
            for r in [radius, radius * 0.6, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    bb = e.boundingBox
                    if (abs(bb.minPoint.y - (face_y + OY) * MM) < 0.02 * MM
                            and abs(bb.maxPoint.y - (face_y + OY) * MM) < 0.02 * MM
                            and bb.minPoint.x > (x0 - 0.05 + OX) * MM
                            and bb.maxPoint.x < (x1 + 0.05 + OX) * MM
                            and not in_plane(e, Z_PCB)):
                        edges.add(e)
                if not edges.count:
                    return 0.0
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(r * MM), True)
                try:
                    root.features.filletFeatures.add(fin)
                    return r
                except:
                    continue
            return 0.0

        def fillet_slot_corners(body, x0, x1, z, box, radius):
            """Round a slot's two bottom corners: the edges at height `z` along its sides at
            X `x0` and `x1`, inside the XY `box`. Returns the radius used, 0.0 if none."""
            for r in [radius, radius * 0.6, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    bb = e.boundingBox
                    if (in_plane(e, z) and in_box(e, box)
                            and any(abs(bb.minPoint.x - (x + OX) * MM) < 0.02 * MM
                                    and abs(bb.maxPoint.x - (x + OX) * MM) < 0.02 * MM
                                    for x in (x0, x1))):
                        edges.add(e)
                if not edges.count:
                    return 0.0
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(r * MM), True)
                try:
                    root.features.filletFeatures.add(fin)
                    return r
                except:
                    continue
            return 0.0

        # =====================================================================
        # BACK TRAY
        # =====================================================================
        tray = solid(rect_sketch(Z_FLOOR_OUT, -OFF, -OFF, BOARD_W + OFF, BOARD_H + OFF),
                     Z_FLOOR_OUT, Z_FLOOR_IN, None)
        tray.name = 'ControllerEnclosure_BackTray'

        def ring(z0, z1, out_inset, in_inset, body):
            """Rectangular ring from z0 to z1, joined to `body`."""
            o = OFF - out_inset
            i = o - in_inset
            sk = root.sketches.add(plane_at(z0))
            L = sk.sketchCurves.sketchLines
            L.addTwoPointRectangle(P(-o, -o), P(BOARD_W + o, BOARD_H + o))
            L.addTwoPointRectangle(P(-i, -i), P(BOARD_W + i, BOARD_H + i))
            full = (BOARD_W + 2 * o) * (BOARD_H + 2 * o)
            for n in range(sk.profiles.count):
                prof = sk.profiles.item(n)
                if prof.areaProperties().area < full * 0.9 * MM * MM:
                    inp = exts.createInput(prof, adsk.fusion.FeatureOperations.JoinFeatureOperation)
                    inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal((z1 - z0) * MM))
                    inp.participantBodies = [body]
                    exts.add(inp)
                    return

        # Wall, up to the board's front face.
        ring(Z_FLOOR_IN, Z_PCB, 0.0, OFF - FIT, tray)

        outer_corners = [(-OFF, -OFF), (BOARD_W + OFF, -OFF),
                         (-OFF, BOARD_H + OFF), (BOARD_W + OFF, BOARD_H + OFF)]
        inner_corners = [(-FIT, -FIT), (BOARD_W + FIT, -FIT),
                         (-FIT, BOARD_H + FIT), (BOARD_W + FIT, BOARD_H + FIT)]
        round_vertical_edges(tray, OUTER_R_XY, outer_corners)
        round_vertical_edges(tray, INNER_R_XY, inner_corners)

        # Corner bosses, floor to the board's back.
        for hx, hy in HOLES:
            mx, my = hx, fy(hy)
            left, low = hx < BOARD_W / 2.0, my < BOARD_H / 2.0
            x0 = -FIT if left else BOARD_W + FIT
            y0 = -FIT if low else BOARD_H + FIT
            x1 = mx + (BOSS_D / 2.0 if left else -BOSS_D / 2.0)
            y1 = my + (BOSS_D / 2.0 if low else -BOSS_D / 2.0)
            tray = solid(rect_sketch(Z_FLOOR_IN, x0, y0, x1, y1),
                         Z_FLOOR_IN, Z_PCB_BACK, tray)

        # Cable slots: two tile ports in the top wall, the jack in the bottom wall.
        top_wall = (fy(0.0) + FIT, fy(0.0) + OFF)                 # model Y, inner to outer
        bottom_wall = (fy(BOARD_H) - OFF, fy(BOARD_H) - FIT)
        for ux in TILE_USB_X:
            cut(rect_sketch(Z_PCB, ux - TILE_SLOT_W / 2.0, top_wall[0] - 0.1,
                            ux + TILE_SLOT_W / 2.0, top_wall[1] + 0.1),
                Z_TILE_SLOT - Z_PCB, tray)
        cut(rect_sketch(Z_PCB, JACK_X - JACK_SLOT_W / 2.0, bottom_wall[0] - 0.1,
                        JACK_X + JACK_SLOT_W / 2.0, bottom_wall[1] + 0.1),
            Z_FLOOR_IN - Z_PCB, tray)

        # Rounding. The plate and board seats stay flat; the slots are rounded separately,
        # and the jack's slot not at all.
        slot_boxes = [(ux - TILE_SLOT_W / 2.0 - 1.0, ux + TILE_SLOT_W / 2.0 + 1.0,
                       top_wall[0] - 1.0, top_wall[1] + 1.0) for ux in TILE_USB_X]
        jack_box = (JACK_X - JACK_SLOT_W / 2.0 - 1.0, JACK_X + JACK_SLOT_W / 2.0 + 1.0,
                    bottom_wall[0] - 1.0, bottom_wall[1] + 1.0)
        tray_r = fillet_edges(tray, EDGE_R_TRAY, keep_flat=(Z_PCB, Z_PCB_BACK, Z_FLOOR_IN),
                              skip=slot_boxes + [jack_box])
        corner_r = [fillet_slot_corners(tray, ux - TILE_SLOT_W / 2.0, ux + TILE_SLOT_W / 2.0,
                                        Z_TILE_SLOT, box, SLOT_CORNER_R)
                    for ux, box in zip(TILE_USB_X, slot_boxes)]
        slot_r = [fillet_slot(tray, ux - TILE_SLOT_W / 2.0, ux + TILE_SLOT_W / 2.0,
                              top_wall[1], NOTCH_R) for ux in TILE_USB_X]
        cav_r = fillet_region(tray, Z_FLOOR_IN, -OFF - 1.0, BOARD_W + OFF + 1.0,
                              -OFF - 1.0, BOARD_H + OFF + 1.0, EDGE_R_CAV, skip=[jack_box])

        # Insert pockets.
        for hx, hy in HOLES:
            cut(circle_sketch(Z_PCB_BACK, hx, fy(hy), INSERT_D), -INSERT_L, tray)
        for hx, hy in HOLES:
            fillet_region(tray, Z_PCB_BACK,
                          hx - INSERT_D / 2.0 - 0.6, hx + INSERT_D / 2.0 + 0.6,
                          fy(hy) - INSERT_D / 2.0 - 0.6, fy(hy) + INSERT_D / 2.0 + 0.6,
                          MOUTH_R)

        # Keyholes.
        for kx, ky in KEYHOLES:
            cut(circle_sketch(Z_FLOOR_OUT, kx, fy(ky), KEY_BIG_D), WALL + 0.2, tray)
            cut(rect_sketch(Z_FLOOR_OUT, kx - KEY_SLOT_D / 2.0, fy(ky),
                            kx + KEY_SLOT_D / 2.0, fy(ky) + KEY_RISE), WALL + 0.2, tray)
            cut(circle_sketch(Z_FLOOR_OUT, kx, fy(ky) + KEY_RISE, KEY_SLOT_D), WALL + 0.2, tray)

        # Locking screw hole, with a head recess on the inside of the floor.
        cut(circle_sketch(Z_FLOOR_OUT, LOCK_HOLE[0], fy(LOCK_HOLE[1]), LOCK_D), WALL + 0.2, tray)
        cut(circle_sketch(Z_FLOOR_IN, LOCK_HOLE[0], fy(LOCK_HOLE[1]), LOCK_REC_D),
            -LOCK_REC_H, tray)

        # Labels on the top face, below each tile slot.
        z_mid = ((Z_TILE_SLOT - NOTCH_R) + (Z_FLOOR_OUT + EDGE_R_TRAY)) / 2.0
        for text, x in TOP_LABELS:
            wall_text(text, x, z_mid, top_wall[1], -1.0, tray)

        # Label on the bottom face, beside the jack's slot.
        z_label = ((Z_FLOOR_OUT + EDGE_R_TRAY) + Z_PCB) / 2.0
        label_lines, x = BOTTOM_LABEL
        for i, text in enumerate(label_lines):
            wall_text(text, x, z_label + ((len(label_lines) - 1) / 2.0 - i) * BOTTOM_LABEL_PITCH,
                      bottom_wall[0], 1.0, tray)

        # =====================================================================
        # FRONT PLATE
        # =====================================================================
        plate = solid(rect_sketch(Z_PLATE_IN, -OFF, -OFF, BOARD_W + OFF, BOARD_H + OFF),
                      Z_PLATE_IN, Z_PLATE_OUT, None)
        plate.name = 'ControllerEnclosure_FrontPlate'

        round_vertical_edges(plate, OUTER_R_XY, outer_corners)
        plate_r = fillet_edges(plate, EDGE_R_PLATE, keep_flat=(Z_PLATE_IN,))

        def opening(cx, cy, hole, slope):
            """Hole through the plate with a sloped rim at the face. `hole` is (width, height)
            for a rounded rectangle, or a list of corner points for a rounded polygon."""
            my = fy(cy)

            def outline(z, grow, r):
                if isinstance(hole[0], tuple):
                    return rounded_poly_sketch(z, cx, my, offset_poly(hole, grow), r)
                return rounded_rect_sketch(z, cx, my, hole[0] + 2 * grow, hole[1] + 2 * grow, r)

            cut(outline(Z_PLATE_OUT, BODY_CLR, HOLE_R), -FRONT_T - 0.2, plate)
            loft_cut(outline(Z_PLATE_OUT, BODY_CLR + slope, HOLE_R + slope),
                     outline(Z_PLATE_OUT - slope, BODY_CLR, HOLE_R), plate)

        for bx, by in BUTTONS:
            opening(bx, by, BTN_TOP, RIM_SLOPE)
        opening(SWITCH[0], SWITCH[1], SW_BODY, RIM_SLOPE)

        # Hollow underside, leaving solid corners under the screws and a column around the mic.
        ci, cb, ch, W, H = CAVITY_INSET, CORNER_BLOCK, CORNER_CHAMFER, BOARD_W, BOARD_H
        hollow = poly_sketch(Z_PLATE_IN, [(x, fy(y)) for x, y in [
            (cb, ci), (W - cb, ci), (W - cb, cb - ch), (W - cb + ch, cb), (W - ci, cb),
            (W - ci, H - cb), (W - cb + ch, H - cb), (W - cb, H - cb + ch), (W - cb, H - ci),
            (cb, H - ci), (cb, H - cb + ch), (cb - ch, H - cb), (ci, H - cb),
            (ci, cb), (cb - ch, cb), (cb, cb - ch)]])
        hollow.sketchCurves.sketchCircles.addByCenterRadius(P(MIC_PORT[0], fy(MIC_PORT[1])),
                                                            MIC_BOSS_D / 2.0 * MM)
        cavity = next(hollow.profiles.item(k) for k in range(hollow.profiles.count)
                      if hollow.profiles.item(k).profileLoops.count == 2)
        inp = exts.createInput(cavity, adsk.fusion.FeatureOperations.CutFeatureOperation)
        inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal((FRONT_T - SKIN) * MM))
        inp.participantBodies = [plate]
        exts.add(inp)

        # IR sensor: body pocket under the skin, round lens window opening out to the face.
        ir_my = fy(SENSOR[1])
        ir_pocket = IR_BODY_H + TOP_CLR
        cut(rounded_rect_sketch(Z_PLATE_IN, SENSOR[0], ir_my, IR_BODY[0] + 2 * BODY_CLR,
                                IR_BODY[1] + 2 * BODY_CLR, HOLE_R), ir_pocket, plate)
        dx, dy = SENSOR[0] + IR_DOME[0], fy(SENSOR[1] + IR_DOME[1])
        skin = Z_PLATE_OUT - (Z_PLATE_IN + ir_pocket)
        loft_cut(circle_sketch(Z_PLATE_OUT, dx, dy, IR_WINDOW_D + 2 * skin),
                 circle_sketch(Z_PLATE_IN + ir_pocket - 0.01, dx, dy, IR_WINDOW_D), plate)

        # Mic hole.
        cut(circle_sketch(Z_PLATE_OUT, MIC_PORT[0], fy(MIC_PORT[1]), MIC_HOLE_D),
            -FRONT_T - 0.2, plate)

        # Screw holes, countersunk.
        for hx, hy in HOLES:
            cut(circle_sketch(Z_PLATE_OUT, hx, fy(hy), SCREW_CLR), -FRONT_T - 0.2, plate)
            cut(circle_sketch(Z_PLATE_OUT, hx, fy(hy), SCREW_HEAD), -CSK_DEPTH, plate)

        # Engravings on the face: HOLD, AUTO, the four arrows and the two sound symbols.
        front_text('HOLD', HOLD_AT[0], HOLD_AT[1], plate)
        front_text('AUTO', AUTO_AT[0], AUTO_AT[1], plate)
        for (ax, ay), direction in ARROWS:
            front_symbol([('poly', arrow_points(direction))], ax, ay, plate, 'arrow ' + direction)
        front_symbol(note_shapes(1.0), NOTE_AT[0], NOTE_AT[1], plate, 'music note')
        nx, ny = NOTE_OFF_AT
        try:
            ring = root.sketches.add(plane_at(Z_PLATE_OUT))
            for d in (RING_D, RING_D - 2 * RING_W):
                ring.sketchCurves.sketchCircles.addByCenterRadius(P(nx, fy(ny)), d / 2.0 * MM)
            annulus = min((ring.profiles.item(i) for i in range(ring.profiles.count)),
                          key=lambda p: p.areaProperties().area)
            engrave(annulus, ENGRAVE_D, plate, 'sound-off ring')
        except:
            skipped.append('sound-off ring')
        front_symbol(note_shapes(RING_NOTE) + [('poly', slash_points())], nx, ny, plate,
                     'sound-off note')

        # Tab that closes the jack's slot above the jack.
        plate = solid(rect_sketch(Z_PLATE_IN, JACK_X - JACK_SLOT_W / 2.0 + TAB_CLR, bottom_wall[0],
                                  JACK_X + JACK_SLOT_W / 2.0 - TAB_CLR, bottom_wall[1]),
                      Z_PLATE_IN, Z_PCB_BACK + TAB_CLR + EXPLODE, plate)

        # Pocket over the jack's pins, a little deeper than the hollow.
        cut(rounded_box_sketch(Z_PLATE_IN, POCKET, POCKET_R), POCKET_D, plate)

        # Hide the sketches.
        for i in range(root.sketches.count):
            root.sketches.item(i).isVisible = False

        ui.messageBox(
            'Controller enclosure built.\n\n'
            'Outer: %.1f x %.1f mm, %.1fmm deep\n'
            'Front plate %.1fmm (%.1fmm over the hollow underside), tray floor %.1fmm\n\n'
            'Above the plate: button caps %.1fmm, switch knob %.1fmm, sensor dome %.1fmm.\n'
            'Jack pins: pocket %.1f deep, %.1fmm of plastic over them.\n\n'
            'Edges rounded: tray %.2f of %.2f asked, cavity floor %.2f of %.2f,\n'
            'plate %.2f of %.2f, tile slots %s of %.2f,\n'
            'tile slot corners %s of %.2f.\n\n%s'
            % (OUTER_W, OUTER_H, Z_PLATE_OUT - Z_FLOOR_OUT, FRONT_T, SKIN, WALL,
               BTN_TOP_Z - FRONT_T, SW_TOP_Z - FRONT_T, IR_TOP_Z - FRONT_T,
               POCKET_D, FRONT_T - POCKET_D,
               tray_r, EDGE_R_TRAY, cav_r, EDGE_R_CAV, plate_r, EDGE_R_PLATE,
               ', '.join('%.2f' % r for r in slot_r), NOTCH_R,
               ', '.join('%.2f' % r for r in corner_r), SLOT_CORNER_R,
               ('Not engraved: ' + ', '.join(sorted(set(skipped)))) if skipped
               else 'All engravings made.'))

    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
