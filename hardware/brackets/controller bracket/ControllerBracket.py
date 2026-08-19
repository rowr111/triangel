# ControllerBracket.py
# Fusion 360 script: wall-mount plate for the Triangel controller board, for running the
# controller and input boards as two separate PCBs.
#
# A flat plate with four standoffs under the board's M3 holes, plus three support pads
# that take the load when a DABAO module is pressed into its sockets. The wall screws go
# through tabs either side of the board rather than underneath it, so they stay reachable
# with the board bolted on and the whole assembly comes off the wall in one piece.
#
# Fastening: M3x8 from the FRONT, down through the board into an M3x4x3 heat-set insert
# pressed into each standoff top - same insert as the triangle brackets and the back tray.
# A blind SCREW_CLR pocket below catches the screw tip, leaving 2mm of solid plate against
# the wall. An M3x6 works too; an M3x10 is too long and bottoms out.
#
# MK1 is a bottom-port mic: its acoustic hole passes through the PCB and fires out the
# back. The plate sits flush to the wall, so an opening under the port would just be a
# dead-end cavity - what the mic breathes through is the open perimeter. So the plate
# carries no lips or walls at all, the gap under the board stays open on all four sides,
# and no standoff or pad stands within MIC_KEEPOUT_R of the port.
#
# Coordinates match controller.kicad_pcb: origin at the board's top-left, X right, Y DOWN.
# fy() flips Y so the model reads the same way you look at the board. Z = 0 is the PCB's
# BACK face, and ALIGN_TO_STEP puts the part in controller.step's frame, so the exported
# board imports at 0,0,0 and lands on the bracket with nothing to reposition.
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.
# All dimensions in mm; Fusion's API is cm internally, so we scale by MM = 0.1.

import adsk.core, adsk.fusion, math, traceback

# ---- Board (from controller.kicad_pcb - do not edit without checking the board) -----
BOARD_W, BOARD_H = 80.0, 76.75
BOARD_T   = 1.6
HOLES     = [(5.0, 5.0), (75.0, 5.0), (5.0, 71.75), (75.0, 71.75)]   # M3 mounting holes
MIC_PORT  = (53.5, 33.71)    # MK1's acoustic hole through the board

# ---- Editable parameters (mm) ------------------------------------------------------
PLATE_T    = 3.0    # plate thickness; it lies flat on the wall so it spans nothing, and
                    # 3.0 still leaves 2mm of solid material under the screw pocket
STANDOFF_H = 6.0    # board back to plate front - the gap the mic breathes through. The
                    # pins need far less; the barrel jack is the deepest, at 2.4mm
STANDOFF_D = 10.0   # 3.2mm of material around the insert, over JLC's 3mm minimum. At
                    # 5mm in from the board edge this lands exactly flush, no overhang

# Support pads (x, y, w, h): they stop the board flexing when a module is pushed into its
# sockets. w == h is a round post, otherwise a rounded rib. The first two sit in the
# 17.78mm corridor between each module's two pin rows; the third is in the clear band
# between the header pads and the USB-C shell tabs. All well clear of the mic.
PADS = [
    (26.0, 22.45,  8.0, 8.0),   # left module, centered on its pin span, 4.1mm clear of it
    (54.0, 22.45,  8.0, 8.0),   # right module, the same, and 7.3mm off the mic port
    (36.0, 46.90, 16.0, 5.0),   # mid-board rib, 1.6mm clear top and bottom
]
PAD_DROP = 0.2      # pad tops sit this far below the standoff tops, so the four screws
                    # still set the plane and the pads only load up when the board flexes

MIC_KEEPOUT_R = 5.0    # keeps a standoff or pad from standing right at the port. Nothing
                       # is meant to sit under it - the plate below is STANDOFF_H down, flat

INSERT_D  = 3.6     # JLC3DP M3x4x3 heat-set insert, same as the triangle brackets
INSERT_L  = 5.0     # JLC requires a 5.0mm-deep hole for that insert, 1mm inside STANDOFF_H
SCREW_CLR = 3.4     # clearance below the pocket, so screw length isn't critical
SCREW_CLR_L = 2.0   # how far that clearance runs on past the insert pocket; an M3x8's tip
                    # lands 1.4mm into it
MOUTH_R   = 0.5     # lead-in round at the pocket mouth, so the insert starts square

WALL_HOLE_D    = 5.0    # takes the usual wall fasteners: #6 through #10 wood screws, M4,
                        # and the screws that come with plastic or self-drilling drywall
                        # anchors. Sized by the HEAD, not the shank - any bigger and a #6
                        # or #8 pan head has under a millimetre of plate to bear on
WALL_HOLE_OUT  = 10.0   # hole center this far past the board edge -> 100mm apart
WALL_SLOT_X    = 2.0    # right hole is slotted this far each way in X, to absorb the slop
                        # in two hand-drilled wall holes. 0 gives two plain round holes
WALL_REC_D     = 12.0   # head recess, on the room-side face. Also takes an M4 or #8 washer
                        # (9-10mm) flush, which is how a small-headed screw or nail gets a
                        # bearing surface it can't pull through
WALL_REC_H     = 1.0    # a third of the plate; the rest is what the head clamps against

# Wall tabs: a lobe around each hole rather than a square ear. The corners of a rectangular
# tab carry nothing, so they are only material and print time.
TAB_RIM     = 3.0   # material left around the head recess; this sets the lobe radius
TAB_BLEND_R = 5.0   # blend where the tab runs back into the board edge

EDGE_R = 1.2        # rounding everywhere except the two mating planes. Held under half of
                    # PLATE_T so the rim keeps a flat face rather than going full bullnose;
                    # the script backs the radius off anyway if a set won't compute

# True places the part in controller.step's frame. False puts the board's top-left at 0,0.
ALIGN_TO_STEP = True
# ------------------------------------------------------------------------------------

MM = 0.1

# Tab lobe: radius set by the recess it has to surround, reaching from the board edge out
# past the far end of the hole. TAB_SPAN is the run from the edge to the outer lobe center,
# so the tab is full width where it meets the plate and tapers only past the hole.
TAB_R     = WALL_REC_D / 2.0 + TAB_RIM
TAB_SPAN  = WALL_HOLE_OUT + WALL_SLOT_X
TAB_REACH = TAB_SPAN + TAB_R          # how far the tab stands past the board edge

# Z = 0 is the PCB's BACK face, matching where KiCad's STEP export puts the board. The
# standoff tops are that same plane - the board rests straight on them.
Z_PCB_BACK  = 0.0
Z_PAD_TOP   = Z_PCB_BACK - PAD_DROP
Z_PLATE_IN  = Z_PCB_BACK - STANDOFF_H     # room-side face, where the standoffs stand up
Z_PLATE_OUT = Z_PLATE_IN - PLATE_T        # against the wall

# controller.step carries the board in KiCad page coordinates with Y negated, so its
# top-left corner lands at (108.5, -55). Shifting by that puts the two in the same frame.
OX = 108.5 if ALIGN_TO_STEP else 0.0
OY = -131.75 if ALIGN_TO_STEP else 0.0


def run(context):
    ui = None
    try:
        app = adsk.core.Application.get()
        ui = app.userInterface
        design = adsk.fusion.Design.cast(app.activeProduct)
        if not design:
            ui.messageBox('Open a new, empty Fusion design first, then run this script.')
            return
        root = design.rootComponent
        exts = root.features.extrudeFeatures

        # Nothing structural is allowed near the acoustic port. Checked before anything is
        # built, so moving a pad in PADS can't quietly wall the mic in.
        worst = min(pad_clearance(MIC_PORT, x, y, w, h)
                    for x, y, w, h in [(hx, hy, STANDOFF_D, STANDOFF_D) for hx, hy in HOLES] + PADS)
        if worst < MIC_KEEPOUT_R:
            ui.messageBox('A standoff or pad comes {:.2f}mm from the mic port, inside the '
                          '{:.1f}mm keep-out.\nMove it or lower MIC_KEEPOUT_R. Nothing was built.'
                          .format(worst, MIC_KEEPOUT_R))
            return

        def fy(y):
            """Board Y (down) -> model Y (up), so the model reads like the board does."""
            return BOARD_H - y

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
            """Rounded-rectangle profile: four lines and four corner arcs."""
            sk = root.sketches.add(plane_at(z))
            lines, arcs = sk.sketchCurves.sketchLines, sk.sketchCurves.sketchArcs
            hw, hh = w / 2.0, h / 2.0
            k = r * (1.0 - math.sqrt(0.5))   # how far a corner arc's midpoint sits in
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

        def circle_sketch(z, cx, cy, dia):
            sk = root.sketches.add(plane_at(z))
            sk.sketchCurves.sketchCircles.addByCenterRadius(P(cx, cy), (dia / 2.0) * MM)
            return sk

        def pad_sketch(z, cx, cy, w, h):
            """A support pad: round post when it's square, otherwise a rounded rib."""
            if abs(w - h) < 1e-9:
                return circle_sketch(z, cx, cy, w)
            return rounded_rect_sketch(z, cx, cy, w, h, min(w, h) / 2.0)

        def solid(sk, z0, z1, body):
            """Extrude a profile; make a new body, or join it to an existing one."""
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
            inp = exts.createInput(sk.profiles.item(0),
                                   adsk.fusion.FeatureOperations.CutFeatureOperation)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(depth * MM))
            inp.participantBodies = [body]
            exts.add(inp)

        def round_vertical_edges(body, radius, corners):
            """Fillet the tall corners standing at the given XY positions."""
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

        def fillet_edges(body, radius, keep_flat=()):
            """Round every edge except those lying in one of `keep_flat`'s planes, which
            have to stay flat and full width - a fillet on the standoff or pad tops would
            leave the board rocking on a ring instead of sitting on a face. The whole set
            goes in as one feature, so one edge too small for `radius` drags the rest down
            to a fallback. Returns the radius that took, 0.0 if none did."""
            for r in [radius, radius * 0.75, radius * 0.5, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    if any(in_plane(e, z) for z in keep_flat):
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

        def fillet_region(body, z, x0, x1, y0, y1, radius):
            """Round the edges sitting in one plane inside one XY box - picked by position
            rather than by shape, so it finds a pocket mouth in a face that fillet_edges
            was told to leave alone."""
            for r in [radius, radius * 0.6]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    bb = e.boundingBox
                    if (in_plane(e, z)
                            and bb.minPoint.x > (x0 + OX) * MM and bb.maxPoint.x < (x1 + OX) * MM
                            and bb.minPoint.y > (y0 + OY) * MM and bb.maxPoint.y < (y1 + OY) * MM):
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

        # --- plate: the board's own footprint, with a tab out each side ---------------
        # The tabs carry the wall screws. Nothing overhangs the board anywhere else, so the
        # air gap under it is open all the way round for the mic.
        bracket = solid(rect_sketch(Z_PLATE_OUT, 0.0, 0.0, BOARD_W, BOARD_H),
                        Z_PLATE_OUT, Z_PLATE_IN, None)
        bracket.name = 'ControllerBracket'

        # Each tab is a lobe of radius TAB_R around its wall hole. Its inner end is centered
        # on the board edge, so the tab is full width where the two merge and narrows only
        # out past the hole, where nothing is carrying load.
        wall_y = fy(BOARD_H / 2.0)
        for cx in (-TAB_SPAN / 2.0, BOARD_W + TAB_SPAN / 2.0):
            bracket = solid(rounded_rect_sketch(Z_PLATE_OUT, cx, wall_y,
                                                TAB_SPAN + 2.0 * TAB_R, 2.0 * TAB_R, TAB_R),
                            Z_PLATE_OUT, Z_PLATE_IN, bracket)

        # Blend each tab into the board edge, before the fillet pass - these inside corners
        # are sharp notches otherwise, and EDGE_R is far too small to soften them.
        round_vertical_edges(bracket, TAB_BLEND_R,
                             [(x, wall_y + dy) for x in (0.0, BOARD_W)
                              for dy in (-TAB_R, TAB_R)])

        # --- standoffs, up to the board's back face ----------------------------------
        for hx, hy in HOLES:
            bracket = solid(circle_sketch(Z_PLATE_IN, hx, fy(hy), STANDOFF_D),
                            Z_PLATE_IN, Z_PCB_BACK, bracket)

        # --- support pads, stopping PAD_DROP short of it ------------------------------
        for px, py, pw, ph in PADS:
            bracket = solid(pad_sketch(Z_PLATE_IN, px, fy(py), pw, ph),
                            Z_PLATE_IN, Z_PAD_TOP, bracket)

        # Everything except the two planes the board sits on.
        fillet_edges(bracket, EDGE_R, keep_flat=(Z_PCB_BACK, Z_PAD_TOP))

        # Insert pockets after the fillet pass, so the general rounding never reaches them.
        for hx, hy in HOLES:
            cut(circle_sketch(Z_PCB_BACK, hx, fy(hy), INSERT_D), -INSERT_L, bracket)
            cut(circle_sketch(Z_PCB_BACK, hx, fy(hy), SCREW_CLR), -(INSERT_L + SCREW_CLR_L), bracket)

        # Lead-in at each mouth. These sit in the standoff-top plane, which fillet_edges
        # skips, so they have to be picked out by position once the pockets exist.
        for hx, hy in HOLES:
            fillet_region(bracket, Z_PCB_BACK,
                          hx - INSERT_D / 2.0 - 0.6, hx + INSERT_D / 2.0 + 0.6,
                          fy(hy) - INSERT_D / 2.0 - 0.6, fy(hy) + INSERT_D / 2.0 + 0.6,
                          MOUTH_R)

        # --- wall holes: left round, right slotted in X, head recessed on the room side -
        # Same circle-slot-circle build as the back tray's keyholes.
        for wx, slot in ((-WALL_HOLE_OUT, 0.0), (BOARD_W + WALL_HOLE_OUT, WALL_SLOT_X)):
            for dia, depth in ((WALL_REC_D, -WALL_REC_H), (WALL_HOLE_D, -(PLATE_T + 0.5))):
                cut(circle_sketch(Z_PLATE_IN, wx - slot, wall_y, dia), depth, bracket)
                if slot > 0.0:
                    cut(rect_sketch(Z_PLATE_IN, wx - slot, wall_y - dia / 2.0,
                                    wx + slot, wall_y + dia / 2.0), depth, bracket)
                    cut(circle_sketch(Z_PLATE_IN, wx + slot, wall_y, dia), depth, bracket)

        ui.messageBox(
            'Done. Created body ControllerBracket.\n\n'
            'Plate {:.0f} x {:.2f} x {:.1f}, standoffs {:.1f} tall.\n'
            'Wall holes {:.0f}mm apart, the right one slotted +/-{:.1f}.\n'
            'Nearest standoff or pad to the mic port: {:.1f}mm.\n\n'
            'Takes 4x M3x4x3 heat-set inserts in the standoff tops and 4x M3x8 screws.'
            .format(BOARD_W + 2 * TAB_REACH, BOARD_H, PLATE_T, STANDOFF_H,
                    BOARD_W + 2 * WALL_HOLE_OUT, WALL_SLOT_X, worst))
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))


def pad_clearance(port, cx, cy, w, h):
    """Gap from the port to a round post or a rounded rib, in board coordinates."""
    r = min(w, h) / 2.0
    dx = max(abs(port[0] - cx) - (w / 2.0 - r), 0.0)
    dy = max(abs(port[1] - cy) - (h / 2.0 - r), 0.0)
    return math.hypot(dx, dy) - r
