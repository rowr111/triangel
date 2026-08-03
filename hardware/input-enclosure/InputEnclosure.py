# InputEnclosure.py
# Fusion 360 script: two-part enclosure for the Triangel input board (80 x 60mm).
#
# BACK TRAY  - mounts to the wall first: two keyholes to hang it, one locking screw
#              through the floor, two cord holes if you'd rather hang it off the apex
#              bracket. Four bosses inside take M3 inserts.
# FRONT PLATE- flat cap carrying every opening. Screws down through the board into
#              those bosses, so one set of four fasteners holds cover and board together.
#
# Component heights were measured from the 3D models, not estimated:
#   slide switch 5.50 (tallest), buttons 5.00, IR sensor 4.20, USB-C 3.25 (back).
# With FRONT_T = 4.0 the switch stands 1.5mm proud, buttons 1.0mm, and the sensor
# sits flush - which is what you want for its line of sight.
#
# Coordinates match firmware/../hardware/input/input_layout.py: origin at the board's
# top-left, X right, Y DOWN. fy() flips Y so the model reads the same way you look at
# the panel. Z = 0 is the PCB's front face, +Z toward the room.
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.
# All dimensions in mm; Fusion's API is cm internally, so we scale by MM = 0.1.

import adsk.core, adsk.fusion, math, traceback

# ---- Board (from input.kicad_pcb - do not edit without checking the board) -----
BOARD_W, BOARD_H = 80.0, 60.0
BOARD_R          = 3.0     # board corner radius
BOARD_T          = 1.6

HOLES   = [(5.0, 5.0), (75.0, 5.0), (5.0, 55.0), (75.0, 55.0)]   # M3 mounting holes
BUTTONS = [(31.0, 11.0), (31.0, 49.0), (12.0, 30.0), (50.0, 30.0), (31.0, 30.0)]
SWITCH  = (64.0, 20.0)     # slide switch, body 12.09 x 3.59
SENSOR  = (64.0, 46.0)     # IR sensor, body 4.09 x 5.09
USB_X   = 40.0             # connector centre; cable leaves through the top edge

# ---- Editable parameters (mm) -------------------------------------------------
FIT        = 0.5    # clearance around the board inside the tray
WALL       = 3.0    # tray wall and floor thickness, uniform. Well over JLCPCB's 1.2mm
                    # minimum printable wall, and thick enough to stay rigid.
FRONT_T    = 3.0    # front plate thickness = how far the outer face sits above the PCB
BACK_CLEAR = 6.0    # gap behind the board; set by the USB-C at 3.25mm

# Openings are shaped, not round. The plate's inner face sits flat on the PCB, so every
# opening has to clear the part's LEADS at board level, not just its body - and a round
# hole would have to clear the diagonal. Measured widths through the plate's thickness:
# button 7.6 x 6.11 (pads spread in X), sensor 6.8 x 7.0, switch 12.0 x 3.5.
# Not square on purpose: X is set by the button's 7.6mm LEAD span, not its 6.1mm body.
# The J-lead pads stick out either side, and the plate lands flat on the PCB, so the
# opening has to clear them and their solder. Y only has to clear the body.
BTN_OPEN_W = 9.5
BTN_OPEN_H = 8.0
SW_SLOT_W  = 14.0   # slide switch slot, overall length (body is 12.0)
SW_SLOT_H  = 5.5    # and 3.5 wide
IR_OPEN_W  = 9.0    # sensor opening, also gives it a clear view
IR_OPEN_H  = 9.0
OPEN_R     = 1.5    # corner radius on the button and sensor openings
CABLE_W    = 11.0   # notch in the top wall for the USB-C plug
USB_H      = 3.25   # how far the connector hangs below the board's back (measured)
CABLE_GAP  = 1.0    # clearance under it. The notch stops here rather than running to the
                    # floor, so raising BACK_CLEAR deepens the tray without deepening the
                    # notch, and the wall below it stays intact.

BOSS_D     = 7.0    # boss outer diameter
INSERT_D   = 3.6    # M3x4x5 heat-set insert, same part as the brackets
INSERT_L   = 5.0    # pocket depth from the boss top
SCREW_CLR  = 3.4    # M3 clearance through the front plate and board
SCREW_HEAD = 6.5    # countersink diameter on the outer face
CSK_DEPTH  = 1.8

KEYHOLES   = [(14.0, 18.0), (66.0, 18.0)]   # symmetric about x=40, clear of the parts behind
KEY_BIG_D  = 8.5    # drops over the screw head
KEY_SLOT_D = 4.5    # screw shank rides in this once it's hung
KEY_RISE   = 8.0    # how far the narrow slot runs toward the top of the panel

# Locking screw, as high as the board's back allows: it sits in the clear band between
# the connector (which ends at y 8.8) and the expander (which starts at y 15.6). Keep it
# off the keyholes' row - three fixings in a line would let the panel rotate about the
# middle one. Use a low-profile head; there is 2.75mm under the connector, not more.
LOCK_HOLE  = (40.0, 12.0)
LOCK_D     = 4.5
# Recess on the INSIDE face, so the screw head sits down in the floor rather than
# standing proud into the cavity. 1.5 deep leaves 1.5mm of floor, over the 1.2 minimum.
LOCK_REC_D = 12.0    # wide and flat, so it suits a nail head, a pan head or a bugle head
LOCK_REC_H = 1.5    # leaves 1.5mm of floor, and 3.25mm of room before the connector

OUTER_R_XY   = BOARD_R + FIT + WALL   # plan-view corner radius, follows the board's
# The cavity's inner corners must match the BOARD's corner radius plus the fit, or the
# board fouls them. Filleting them at OUTER_R_XY would eat 0.53mm into each corner.
INNER_R_XY   = BOARD_R + FIT
EDGE_R_TRAY  = 3.0  # aggressive rounding on every other tray edge, inside and out
EDGE_R_PLATE = 1.9  # the plate is only FRONT_T thick, so this is a full bullnose
RIM_R        = 0.8  # lead-in round on the button and sensor openings
NOTCH_R      = 1.0  # milder round on the cable opening - the wall is only WALL thick

# True assembly at 0: the plate's inner face lands on the board, whose back rests on the
# boss tops BOARD_T below - so the gap between them is exactly the board's thickness.
# Raise this only to pull the plate away for a look inside; the parts don't change.
EXPLODE      = 0.0
# ------------------------------------------------------------------------------

MM = 0.1

OUTER_W = BOARD_W + 2 * (FIT + WALL)
OUTER_H = BOARD_H + 2 * (FIT + WALL)

# Z = 0 is the PCB's BACK face, matching where KiCad's STEP export puts the board, so an
# imported board drops in at 0,0,0 with nothing to reposition.
Z_PCB_BACK  = 0.0              # PCB back face - rests on the bosses
Z_PCB       = BOARD_T          # PCB front face - the plate lands here
Z_FLOOR_IN  = Z_PCB_BACK - BACK_CLEAR
Z_FLOOR_OUT = Z_FLOOR_IN - WALL
Z_CABLE_BOT = Z_PCB_BACK - USB_H - CABLE_GAP   # bottom of the cable notch

# The plate normally lands straight on the tray wall and the board. EXPLODE lifts it
# clear so you can see the board sitting in the tray - view only, set it back to 0
# before exporting anything to print.
Z_PLATE_IN  = Z_PCB + EXPLODE
Z_PLATE_OUT = Z_PCB + FRONT_T + EXPLODE


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

        def fy(y):
            """Board Y (down) -> model Y (up), so the model reads like the panel does."""
            return BOARD_H - y

        # Tray and plate share the board's frame, offset by the wall + fit on each side.
        OFF = FIT + WALL

        def plane_at(z):
            if abs(z) < 1e-9:
                return root.xYConstructionPlane
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * MM))
            return root.constructionPlanes.add(pin)

        def P(x, y):
            return adsk.core.Point3D.create(x * MM, y * MM, 0)

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
            """Fillet the tall corners standing at the given XY positions. Outer and inner
            corners need different radii, so they can't be done in one pass."""
            for r in [radius, radius * 0.75, radius * 0.5]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    g = e.geometry
                    if isinstance(g, adsk.core.Line3D):
                        a, b = g.startPoint, g.endPoint
                        if abs(a.x - b.x) < 1e-6 and abs(a.y - b.y) < 1e-6:
                            if any(abs(a.x - cx * MM) < 0.05 * MM
                                   and abs(a.y - cy * MM) < 0.05 * MM for cx, cy in corners):
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
            """Round every edge except those lying in a mating plane. Those faces have to
            stay flat and full width - a fillet on the wall top would eat the surface the
            front plate sits on, and one on the boss tops would leave the board rocking."""
            for r in [radius, radius * 0.75, radius * 0.5, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    if not any(in_plane(e, z) for z in keep_flat):
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

        def fillet_region(body, z, x0, x1, y0, y1, radius):
            """Round one opening's rim, picked by where its edges sit rather than by
            shape - the switch slot is two lines and two arcs, so a radius test misses it."""
            for r in [radius, radius * 0.6, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    bb = e.boundingBox
                    if (in_plane(e, z)
                            and bb.minPoint.x > x0 * MM and bb.maxPoint.x < x1 * MM
                            and bb.minPoint.y > y0 * MM and bb.maxPoint.y < y1 * MM):
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

        def fillet_notch(body, radius):
            """Soften the cable opening, but leave its top edge sharp - that sits on the
            wall face the board and front plate land on."""
            x0 = (USB_X - CABLE_W / 2.0 - 1.0) * MM
            x1 = (USB_X + CABLE_W / 2.0 + 1.0) * MM
            y0 = (fy(0.0) + FIT - 1.0) * MM
            for r in [radius, radius * 0.6, radius * 0.3]:
                edges = adsk.core.ObjectCollection.create()
                for i in range(body.edges.count):
                    e = body.edges.item(i)
                    bb = e.boundingBox
                    if (bb.minPoint.x > x0 and bb.maxPoint.x < x1
                            and bb.minPoint.y > y0 and not in_plane(e, Z_PCB)):
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

        # =====================================================================
        # BACK TRAY
        # =====================================================================
        tray = solid(rect_sketch(Z_FLOOR_OUT, -OFF, -OFF, BOARD_W + OFF, BOARD_H + OFF),
                     Z_FLOOR_OUT, Z_FLOOR_IN, None)
        tray.name = 'InputEnclosure_BackTray'

        # Perimeter wall, up to where the front plate lands.
        def ring(z0, z1, out_inset, in_inset, body):
            """Rectangular ring: outer boundary set back from the footprint by out_inset,
            inner boundary set in from it by in_inset."""
            o = OFF - out_inset
            i = o - in_inset
            sk = root.sketches.add(plane_at(z0))
            L = sk.sketchCurves.sketchLines
            L.addTwoPointRectangle(P(-o, -o), P(BOARD_W + o, BOARD_H + o))
            L.addTwoPointRectangle(P(-i, -i), P(BOARD_W + i, BOARD_H + i))
            full = (BOARD_W + 2 * o) * (BOARD_H + 2 * o)
            for n in range(sk.profiles.count):
                prof = sk.profiles.item(n)
                # the ring is the profile smaller than the whole rectangle
                if prof.areaProperties().area < full * 0.9 * MM * MM:
                    inp = exts.createInput(prof, adsk.fusion.FeatureOperations.JoinFeatureOperation)
                    inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal((z1 - z0) * MM))
                    inp.participantBodies = [body]
                    exts.add(inp)
                    return

        # One wall, full thickness, all the way to the board's front face. The board drops
        # into the cavity and sits on the bosses; the plate then caps it flat.
        ring(Z_FLOOR_IN, Z_PCB, 0.0, OFF - FIT, tray)

        # Corners first, while the shape is still simple. Outer and inner get different
        # radii - the inner ones have to let the board's own 3mm corners drop in.
        outer_corners = [(-OFF, -OFF), (BOARD_W + OFF, -OFF),
                         (-OFF, BOARD_H + OFF), (BOARD_W + OFF, BOARD_H + OFF)]
        inner_corners = [(-FIT, -FIT), (BOARD_W + FIT, -FIT),
                         (-FIT, BOARD_H + FIT), (BOARD_W + FIT, BOARD_H + FIT)]
        round_vertical_edges(tray, OUTER_R_XY, outer_corners)
        round_vertical_edges(tray, INNER_R_XY, inner_corners)

        # Bosses: floor up to the back of the board, so the board rests on them.
        # Each boss runs out to the two walls it sits beside, making a solid corner pad
        # rather than a lone pillar - stiffer, and it prints without support. Built before
        # the fillet pass so their bases get the same aggressive rounding as everything else.
        for hx, hy in HOLES:
            mx, my = hx, fy(hy)
            left, low = hx < BOARD_W / 2.0, my < BOARD_H / 2.0
            x0 = -FIT if left else BOARD_W + FIT
            y0 = -FIT if low else BOARD_H + FIT
            x1 = mx + (BOSS_D / 2.0 if left else -BOSS_D / 2.0)
            y1 = my + (BOSS_D / 2.0 if low else -BOSS_D / 2.0)
            tray = solid(rect_sketch(Z_FLOOR_IN, x0, y0, x1, y1),
                         Z_FLOOR_IN, Z_PCB_BACK, tray)

        # Cable notch through the top wall, also before the fillet pass. It stops just
        # under the connector rather than running down to the floor.
        cut(rect_sketch(Z_PCB, USB_X - CABLE_W / 2.0, fy(0.0) + FIT - 0.1,
                        USB_X + CABLE_W / 2.0, fy(0.0) + OFF + 0.1),
            Z_CABLE_BOT - Z_PCB, tray)

        # Everything except the two mating planes: the wall top the plate lands on, and
        # the boss tops the board rests on. Both have to stay flat and full width.
        fillet_edges(tray, EDGE_R_TRAY, keep_flat=(Z_PCB, Z_PCB_BACK))
        fillet_notch(tray, NOTCH_R)

        # Insert pockets last, so the pocket mouths stay crisp.
        for hx, hy in HOLES:
            cut(circle_sketch(Z_PCB_BACK, hx, fy(hy), INSERT_D), -INSERT_L, tray)

        # Keyholes: big circle to drop over the screw head, narrow slot above it so the
        # panel hangs down onto the shank.
        for kx, ky in KEYHOLES:
            cut(circle_sketch(Z_FLOOR_OUT, kx, fy(ky), KEY_BIG_D), WALL + 0.2, tray)
            cut(rect_sketch(Z_FLOOR_OUT, kx - KEY_SLOT_D / 2.0, fy(ky),
                            kx + KEY_SLOT_D / 2.0, fy(ky) + KEY_RISE), WALL + 0.2, tray)
            cut(circle_sketch(Z_FLOOR_OUT, kx, fy(ky) + KEY_RISE, KEY_SLOT_D), WALL + 0.2, tray)

        # Locking screw. No separate cord holes - the keyholes take a strap just as well.
        # The recess is on the inside face: without it the head would stand ~3mm into the
        # cavity and graze the connector, which hangs 3.25mm off the board's back.
        cut(circle_sketch(Z_FLOOR_OUT, LOCK_HOLE[0], fy(LOCK_HOLE[1]), LOCK_D), WALL + 0.2, tray)
        cut(circle_sketch(Z_FLOOR_IN, LOCK_HOLE[0], fy(LOCK_HOLE[1]), LOCK_REC_D),
            -LOCK_REC_H, tray)

        # =====================================================================
        # FRONT PLATE
        # =====================================================================
        plate = solid(rect_sketch(Z_PLATE_IN, -OFF, -OFF, BOARD_W + OFF, BOARD_H + OFF),
                      Z_PLATE_IN, Z_PLATE_OUT, None)
        plate.name = 'InputEnclosure_FrontPlate'

        round_vertical_edges(plate, OUTER_R_XY, outer_corners)
        # Everything except the inner face, which lands on the tray wall and the board.
        fillet_edges(plate, EDGE_R_PLATE, keep_flat=(Z_PLATE_IN,))

        for bx, by in BUTTONS:
            cut(rounded_rect_sketch(Z_PLATE_OUT, bx, fy(by), BTN_OPEN_W, BTN_OPEN_H, OPEN_R),
                -FRONT_T - 0.2, plate)

        # Switch slot, SW_SLOT_W long overall: the straight part is shortened by the end
        # radius at each end, so adding the round ends brings it back to SW_SLOT_W.
        sx, sy = SWITCH
        half = (SW_SLOT_W - SW_SLOT_H) / 2.0
        cut(rect_sketch(Z_PLATE_OUT, sx - half, fy(sy) - SW_SLOT_H / 2.0,
                        sx + half, fy(sy) + SW_SLOT_H / 2.0), -FRONT_T - 0.2, plate)
        for end in (-1, 1):
            cut(circle_sketch(Z_PLATE_OUT, sx + end * half, fy(sy), SW_SLOT_H),
                -FRONT_T - 0.2, plate)

        cut(rounded_rect_sketch(Z_PLATE_OUT, SENSOR[0], fy(SENSOR[1]),
                                IR_OPEN_W, IR_OPEN_H, OPEN_R), -FRONT_T - 0.2, plate)

        # Screw holes through to the bosses, countersunk on the face.
        for hx, hy in HOLES:
            cut(circle_sketch(Z_PLATE_OUT, hx, fy(hy), SCREW_CLR), -FRONT_T - 0.2, plate)
            cut(circle_sketch(Z_PLATE_OUT, hx, fy(hy), SCREW_HEAD), -CSK_DEPTH, plate)

        # No cable notch here: the connector is on the board's BACK, so it and the cable
        # sit entirely below this plate. Only the tray needs an opening.

        # Lead-in round on every opening, so a fingertip is guided in rather than
        # catching the rim. Picked by region, since none of these are plain circles.
        M = 1.0
        for bx, by in BUTTONS:
            fillet_region(plate, Z_PLATE_OUT,
                          bx - BTN_OPEN_W / 2.0 - M, bx + BTN_OPEN_W / 2.0 + M,
                          fy(by) - BTN_OPEN_H / 2.0 - M, fy(by) + BTN_OPEN_H / 2.0 + M, RIM_R)
        fillet_region(plate, Z_PLATE_OUT,
                      SENSOR[0] - IR_OPEN_W / 2.0 - M, SENSOR[0] + IR_OPEN_W / 2.0 + M,
                      fy(SENSOR[1]) - IR_OPEN_H / 2.0 - M, fy(SENSOR[1]) + IR_OPEN_H / 2.0 + M,
                      RIM_R)
        fillet_region(plate, Z_PLATE_OUT,
                      sx - SW_SLOT_W / 2.0 - M, sx + SW_SLOT_W / 2.0 + M,
                      fy(sy) - SW_SLOT_H / 2.0 - M, fy(sy) + SW_SLOT_H / 2.0 + M, RIM_R)

        ui.messageBox(
            'Input enclosure built.\n\n'
            'Outer: %.1f x %.1f mm, %.1fmm deep\n'
            'Front plate %.1fmm, tray floor %.1fmm\n\n'
            'Buttons stand %.1fmm proud, switch %.1fmm, sensor %.1fmm.'
            % (OUTER_W, OUTER_H, Z_PLATE_OUT - Z_FLOOR_OUT, FRONT_T, WALL,
               5.00 - FRONT_T, 5.50 - FRONT_T, 4.20 - FRONT_T))

    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
