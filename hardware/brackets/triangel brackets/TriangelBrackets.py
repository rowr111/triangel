# TriangelBrackets.py
# Fusion 360 script: builds the two triangle-fixture brackets automatically.
#   Bracket_6way : interior junction, 6 tiles meet
#   Bracket_3way : outer-edge junction, 3 tiles meet
# Both are created as separate BODIES in the root component (works in a Part
# document, which allows only one component).
#
# Fastening: M3 screw enters from the FRONT (through the tile), threads into an
# M3 heat-set insert pressed into the bracket from the tile side. Each hole is a
# Ø3.6mm x 5mm hole (open on the tile-contact face) for the JLC M3x4x3 insert.
# The plate is 5mm (= the insert depth), so the hole runs straight through with a
# flat back and the screw shaft passes out the back (screw length is uncritical).
# (Thicker plate -> blind pocket + Ø3.4mm back clearance; thinner -> a boss is added.)
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.
# All dimensions in mm; Fusion's API is cm internally, so we scale by MM=0.1.

import adsk.core, adsk.fusion, traceback, math

# ---- Editable parameters (mm) -------------------------------------------------
R_HOLE       = 28.60   # bolt-circle radius: center to each hole
PLATE_R      = 34.0    # plate radius (3.6mm insert wall, over JLC 3mm min); binding part is the USB-C J1/J2 at ~35mm body edge -> +1.0mm. Needs the moved-cap board layout.
PLATE_T      = 5.0     # plate thickness = insert depth: the Ø3.6 pocket now goes fully through (no floor), flat back; thinner = less stiff (~thickness^3)

INSERT_HOLE_D = 3.6    # JLC3DP M3x4x3 insert: Ø3.6 hole (from their threaded-insert service table)
INSERT_DEPTH  = 5.0    # JLC requires a 5.0mm-deep hole for that insert
SCREW_CLEAR_D = 3.4    # clearance hole below the insert pocket (M3 screw shaft can pass / protrude)
BOSS_D        = 8.0    # insert boss outer diameter (only used if the plate is set thinner than the insert)
BOSS_EXT      = 4.0    # how far a boss would extend down the back (only if plate < insert depth)

RIDGE_H   = 2.0        # ridge height above the plate (spans the 1.6mm PCB edge + lead-in)
RIDGE_W   = 1.4        # ridge width (sits in the 2mm gap between tiles)
RIDGE_R0  = 0.0        # ridge inner radius (0 = rails meet at the center, forming a star hub)
RIDGE_R1  = 32.0       # ridge outer radius (runs out into the gap)

PART_GAP  = 90.0       # spacing between the two parts in the viewport

EDGE_FILLET  = 0.5     # round on the bigger edges (plate rim, ridge crests, hole mouths)
EDGE_MIN_LEN = 4.0     # only fillet edges at least this long (skips tiny internal bits)
# ------------------------------------------------------------------------------

MM = 0.1  # mm -> cm (Fusion internal units)


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

        make_bracket(root, 'Bracket_6way',
                     hole_angles=[30, 90, 150, 210, 270, 330],
                     ridge_angles=[0, 60, 120, 180, 240, 300],
                     half=False, x_offset=0.0)

        make_bracket(root, 'Bracket_3way',
                     hole_angles=[30, 90, 150],
                     ridge_angles=[60, 120],
                     half=True, x_offset=PART_GAP)

        ui.messageBox('Done. Created bodies Bracket_6way and Bracket_3way.\n'
                      'Insert pockets open on the tile-contact face; bigger edges rounded 0.5mm.')
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))


def make_bracket(root, name, hole_angles, ridge_angles, half, x_offset):
    exts = root.features.extrudeFeatures
    sketches = root.sketches
    XY = root.xYConstructionPlane

    def PT(x, y):  # mm point with the part's X offset applied
        return adsk.core.Point3D.create((x + x_offset) * MM, y * MM, 0)

    def hole_circles(sketch, dia):
        for a in hole_angles:
            r = math.radians(a)
            sketch.sketchCurves.sketchCircles.addByCenterRadius(
                PT(R_HOLE * math.cos(r), R_HOLE * math.sin(r)), (dia / 2.0) * MM)

    def profiles(sketch):
        col = adsk.core.ObjectCollection.create()
        for i in range(sketch.profiles.count):
            col.add(sketch.profiles.item(i))
        return col

    def val(mm):
        return adsk.core.ValueInput.createByReal(mm * MM)

    # --- 1) plate (up from XY: z = 0 .. PLATE_T) ---
    sk = sketches.add(XY)
    if not half:
        sk.sketchCurves.sketchCircles.addByCenterRadius(PT(0, 0), PLATE_R * MM)
    else:
        sk.sketchCurves.sketchArcs.addByThreePoints(PT(PLATE_R, 0), PT(0, PLATE_R), PT(-PLATE_R, 0))
        sk.sketchCurves.sketchLines.addByTwoPoints(PT(-PLATE_R, 0), PT(PLATE_R, 0))

    plate_feat = exts.addSimple(sk.profiles.item(0), val(PLATE_T),
                                adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
    bracket = plate_feat.bodies.item(0)
    bracket.name = name  # this specific body is threaded through every later step

    # --- 2) insert bosses on the back -- only when the plate is too thin to hold
    #        the insert on its own; a thick enough plate gives a clean flat back ---
    need_boss = PLATE_T < INSERT_DEPTH
    if need_boss:
        skb = sketches.add(XY)
        hole_circles(skb, BOSS_D)
        boss_in = exts.createInput(profiles(skb), adsk.fusion.FeatureOperations.JoinFeatureOperation)
        boss_in.setDistanceExtent(False, val(-BOSS_EXT))
        boss_in.participantBodies = [bracket]
        exts.add(boss_in)

    # construction plane on the tile-contact (top) face
    pin = root.constructionPlanes.createInput()
    pin.setByOffset(XY, val(PLATE_T))
    top_plane = root.constructionPlanes.add(pin)

    # --- 3) screw clearance holes: Ø3.4 through everything ---
    skc = sketches.add(top_plane)
    hole_circles(skc, SCREW_CLEAR_D)
    cut_in = exts.createInput(profiles(skc), adsk.fusion.FeatureOperations.CutFeatureOperation)
    cut_in.setDistanceExtent(False, val(-(PLATE_T + (BOSS_EXT if need_boss else 0.0) + 0.5)))
    cut_in.participantBodies = [bracket]
    exts.add(cut_in)

    # --- 4) heat-set insert pockets: Ø4.0 x INSERT_DEPTH, open on the top face ---
    skp = sketches.add(top_plane)
    hole_circles(skp, INSERT_HOLE_D)
    pk_in = exts.createInput(profiles(skp), adsk.fusion.FeatureOperations.CutFeatureOperation)
    pk_in.setDistanceExtent(False, val(-INSERT_DEPTH))
    pk_in.participantBodies = [bracket]
    exts.add(pk_in)

    # --- 5) guide ridges (up from top face, in the gap directions) ---
    hw = RIDGE_W / 2.0
    skr = sketches.add(top_plane)
    lines = skr.sketchCurves.sketchLines

    def poly(points):
        pp = [PT(x, y) for (x, y) in points]
        for i in range(len(pp)):
            lines.addByTwoPoints(pp[i], pp[(i + 1) % len(pp)])

    if half:
        # 3-way: two constant-width rails, each cut off FLUSH with the plate's flat
        # edge (y = 0). Full width right to the end -- no taper; the sharp corners at
        # the flush cut are left for a hand chamfer into the point.
        def rail_clipped(a):
            u = (math.cos(a), math.sin(a))
            n = (-math.sin(a), math.cos(a))
            farP = (RIDGE_R1 * u[0] + hw * n[0], RIDGE_R1 * u[1] + hw * n[1])
            farM = (RIDGE_R1 * u[0] - hw * n[0], RIDGE_R1 * u[1] - hw * n[1])
            baseP = (hw * n[0] - hw * n[1] / u[1] * u[0], 0.0)    # + edge meets y = 0
            baseM = (-hw * n[0] + hw * n[1] / u[1] * u[0], 0.0)   # - edge meets y = 0
            return [farP, farM, baseM, baseP]

        poly(rail_clipped(math.radians(ridge_angles[0])))
        poly(rail_clipped(math.radians(ridge_angles[1])))
    else:
        # 6-way: independent rails running to the center -> symmetric star hub.
        for a in ridge_angles:
            r = math.radians(a)
            u = (math.cos(r), math.sin(r))
            t = (-math.sin(r), math.cos(r))
            poly([
                (RIDGE_R0 * u[0] + hw * t[0], RIDGE_R0 * u[1] + hw * t[1]),
                (RIDGE_R1 * u[0] + hw * t[0], RIDGE_R1 * u[1] + hw * t[1]),
                (RIDGE_R1 * u[0] - hw * t[0], RIDGE_R1 * u[1] - hw * t[1]),
                (RIDGE_R0 * u[0] - hw * t[0], RIDGE_R0 * u[1] - hw * t[1]),
            ])

    ridge_in = exts.createInput(profiles(skr), adsk.fusion.FeatureOperations.JoinFeatureOperation)
    ridge_in.setDistanceExtent(False, val(RIDGE_H))
    ridge_in.participantBodies = [bracket]
    exts.add(ridge_in)

    # --- 6) soften: round the bigger edges (plate rim, ridge crests, hole mouths) ---
    # Blanket 0.5mm on every edge at least EDGE_MIN_LEN long, which skips the tiny
    # star-center slivers. Backs off the radius if a set won't compute.
    for rr in (EDGE_FILLET, 0.4, 0.3):
        soft = adsk.core.ObjectCollection.create()
        for i in range(bracket.edges.count):
            e = bracket.edges.item(i)
            if e.length >= EDGE_MIN_LEN * MM:
                soft.add(e)
        if soft.count == 0:
            break
        fin = root.features.filletFeatures.createInput()
        fin.addConstantRadiusEdgeSet(soft, val(rr), True)
        try:
            root.features.filletFeatures.add(fin)
            break
        except:
            continue
