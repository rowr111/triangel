# frame_bracket.py
# Fusion 360 script: the outer-edge (3-way) bracket with a tab that holds the frame.
#
# The bracket itself is the same as Bracket_3way in TriangelBrackets.py: half-disc plate,
# three M3 heat-set insert holes opening on the tile-contact face, two guide ridges. The
# tab sticks straight out of the plate's flat edge, flush with the plate's back, into the
# slot in a frame piece. An M3x8 button- or pan-head screw goes in from the back, through
# the tab, into the insert in the slot's ceiling.
#
# The back is pocketed to save material: solid rim, solid ring around each insert, solid
# block behind the tab, and a thin floor under the tile face, which stays flat.
#
# Bracket settings must match TriangelBrackets.py; tab settings must match frame.py.
#
# Axes: X along the flat edge, +Y into the tiles, Z=0 the back, Z=PLATE_T the tile face.
# Built that way, then turned and moved into the slot of the straight piece from frame.py.
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.
# All dimensions in mm; Fusion's API is cm internally, so we scale by MM=0.1.

import adsk.core, adsk.fusion, traceback, math

# ---- Editable parameters (mm) -------------------------------------------------
R_HOLE       = 28.60   # bolt-circle radius: center to each hole
PLATE_R      = 34.0    # plate radius
PLATE_T      = 5.0     # plate thickness = insert depth, so the insert hole runs straight through

INSERT_HOLE_D = 3.6    # JLC3DP M3x4x3 insert: Ø3.6 hole
SCREW_CLEAR_D = 3.4    # M3 screw clearance

RIDGE_H   = 2.0        # ridge height above the plate
RIDGE_W   = 1.4        # ridge width (sits in the 2mm gap between tiles)
RIDGE_R1  = 32.0       # ridge outer radius

EDGE_FILLET  = 0.5     # round on the bigger edges (plate rim, ridge crests, hole mouths, tab)
EDGE_MIN_LEN = 4.0     # only fillet edges at least this long (skips tiny internal bits)

TAB_W      = 10.0      # tab width (X)
TAB_T      = 3.0       # tab thickness, flush with the plate's back
TAB_REACH  = 14.0      # out from the flat edge: 1mm to the frame's inner face + its 13mm slot
SCREW_Y    = 9.0       # screw hole, out from the flat edge: 1mm + the frame's insert at 8mm
TAB_ROOT_R = 1.0       # fillet in the inside corners where the tab meets the plate

RIM        = 2.5       # solid edge left around the back pocket
FLOOR_T    = 2.0       # plate left under the tile face, over the pocket
BOSS_WALL  = 3.0       # solid around each insert hole (JLC's minimum insert wall)
TAB_BLOCK  = 6.0       # solid behind the tab, in from the flat edge

# Placement, so the bracket sits in a straight piece's slot when both scripts run in one design
PITCH      = 103.46    # edge bracket spacing; the bracket is PITCH / 2 along the piece
INNER_OFF  = 1.0       # the frame's inner face is this far out from the flat edge
# ------------------------------------------------------------------------------

MM = 0.1  # mm -> cm (Fusion internal units)

HOLE_ANGLES  = [30, 90, 150]
RIDGE_ANGLES = [60, 120]


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
        sketches = root.sketches
        XY = root.xYConstructionPlane
        FO = adsk.fusion.FeatureOperations

        def PT(x, y):
            return adsk.core.Point3D.create(x * MM, y * MM, 0)

        def val(mm):
            return adsk.core.ValueInput.createByReal(mm * MM)

        def profiles(sketch):
            col = adsk.core.ObjectCollection.create()
            for i in range(sketch.profiles.count):
                col.add(sketch.profiles.item(i))
            return col

        def hole_circles(sketch, dia):
            for a in HOLE_ANGLES:
                r = math.radians(a)
                sketch.sketchCurves.sketchCircles.addByCenterRadius(
                    PT(R_HOLE * math.cos(r), R_HOLE * math.sin(r)), (dia / 2.0) * MM)

        def extrude(sketch, distance, operation, body=None):
            inp = exts.createInput(profiles(sketch), operation)
            inp.setDistanceExtent(False, val(distance))
            if body is not None:
                inp.participantBodies = [body]
            return exts.add(inp)

        def poly(sketch, points):
            lines = sketch.sketchCurves.sketchLines
            pp = [PT(x, y) for (x, y) in points]
            for i in range(len(pp)):
                lines.addByTwoPoints(pp[i], pp[(i + 1) % len(pp)])

        # --- plate: half disc, flat edge on y = 0 ---
        sk = sketches.add(XY)
        sk.sketchCurves.sketchArcs.addByThreePoints(PT(PLATE_R, 0), PT(0, PLATE_R), PT(-PLATE_R, 0))
        sk.sketchCurves.sketchLines.addByTwoPoints(PT(-PLATE_R, 0), PT(PLATE_R, 0))
        bracket = extrude(sk, PLATE_T, FO.NewBodyFeatureOperation).bodies.item(0)
        bracket.name = 'Bracket_3way_tab'

        # --- tab: out the flat edge (-Y), flush with the back; starts 1mm inside the plate to join ---
        skt = sketches.add(XY)
        poly(skt, [(-TAB_W / 2.0, 1.0), (TAB_W / 2.0, 1.0), (TAB_W / 2.0, -TAB_REACH), (-TAB_W / 2.0, -TAB_REACH)])
        extrude(skt, TAB_T, FO.JoinFeatureOperation, bracket)

        pin = root.constructionPlanes.createInput()
        pin.setByOffset(XY, val(PLATE_T))
        top_plane = root.constructionPlanes.add(pin)

        # --- insert holes: Ø3.6 straight through the plate, opening on the tile face ---
        skp = sketches.add(top_plane)
        hole_circles(skp, INSERT_HOLE_D)
        extrude(skp, -(PLATE_T + 0.5), FO.CutFeatureOperation, bracket)

        # --- screw hole through the tab ---
        sks = sketches.add(XY)
        sks.sketchCurves.sketchCircles.addByCenterRadius(PT(0, -SCREW_Y), (SCREW_CLEAR_D / 2.0) * MM)
        inp = exts.createInput(profiles(sks), FO.CutFeatureOperation)
        inp.setSymmetricExtent(val(2.0 * (TAB_T + 1.0)), True)
        inp.participantBodies = [bracket]
        exts.add(inp)

        # --- guide ridges: two constant-width rails, cut off flush with the flat edge (y = 0) ---
        hw = RIDGE_W / 2.0
        skr = sketches.add(top_plane)
        for a in RIDGE_ANGLES:
            r = math.radians(a)
            u = (math.cos(r), math.sin(r))
            n = (-math.sin(r), math.cos(r))
            poly(skr, [(RIDGE_R1 * u[0] + hw * n[0], RIDGE_R1 * u[1] + hw * n[1]),
                       (RIDGE_R1 * u[0] - hw * n[0], RIDGE_R1 * u[1] - hw * n[1]),
                       (-hw * n[0] + hw * n[1] / u[1] * u[0], 0.0),
                       (hw * n[0] - hw * n[1] / u[1] * u[0], 0.0)])
        extrude(skr, RIDGE_H, FO.JoinFeatureOperation, bracket)

        # --- back pocket: built as a cutting body (the plate shrunk by RIM, minus a ring
        #     around each insert and a block behind the tab), then cut from the back ---
        pin = root.constructionPlanes.createInput()
        pin.setByOffset(XY, val(-1.0))
        below = root.constructionPlanes.add(pin)
        pocket_h = PLATE_T - FLOOR_T + 1.0       # from 1mm below the back up to the floor

        def combine(target, tool, operation):
            tools = adsk.core.ObjectCollection.create()
            tools.add(tool)
            cin = root.features.combineFeatures.createInput(target, tools)
            cin.operation = operation
            root.features.combineFeatures.add(cin)

        ri = PLATE_R - RIM
        xr = math.sqrt(ri * ri - RIM * RIM)
        skk = sketches.add(below)
        skk.sketchCurves.sketchArcs.addByThreePoints(PT(xr, RIM), PT(0, ri), PT(-xr, RIM))
        skk.sketchCurves.sketchLines.addByTwoPoints(PT(-xr, RIM), PT(xr, RIM))
        pocket = extrude(skk, pocket_h, FO.NewBodyFeatureOperation).bodies.item(0)

        skb = sketches.add(below)
        hole_circles(skb, INSERT_HOLE_D + 2.0 * BOSS_WALL)
        bosses = exts.createInput(profiles(skb), FO.NewBodyFeatureOperation)
        bosses.setDistanceExtent(False, val(pocket_h + 1.0))
        made = exts.add(bosses).bodies
        for b in [made.item(i) for i in range(made.count)]:   # copy first: each cut removes its body from `made`
            combine(pocket, b, FO.CutFeatureOperation)

        skn = sketches.add(below)
        bw = TAB_W / 2.0 + RIM
        poly(skn, [(-bw, -1.0), (bw, -1.0), (bw, TAB_BLOCK), (-bw, TAB_BLOCK)])
        combine(pocket, extrude(skn, pocket_h + 1.0, FO.NewBodyFeatureOperation).bodies.item(0),
                FO.CutFeatureOperation)

        solid_vol = bracket.physicalProperties.volume
        combine(bracket, pocket, FO.CutFeatureOperation)
        pocketed_vol = bracket.physicalProperties.volume

        # --- tab root: fillet the inside corners where the tab meets the plate's flat edge ---
        root_edges = adsk.core.ObjectCollection.create()
        for i in range(bracket.edges.count):
            e = bracket.edges.item(i)
            bb = e.boundingBox
            if (abs(bb.minPoint.y) < 0.001 and abs(bb.maxPoint.y) < 0.001
                    and bb.maxPoint.z <= (TAB_T + 0.01) * MM
                    and abs(bb.minPoint.x) <= (TAB_W / 2.0 + 0.01) * MM
                    and abs(bb.maxPoint.x) <= (TAB_W / 2.0 + 0.01) * MM):
                root_edges.add(e)
        root_done, root_faces = False, set()
        fin = root.features.filletFeatures.createInput()
        fin.addConstantRadiusEdgeSet(root_edges, val(TAB_ROOT_R), False)
        try:
            root_fillet = root.features.filletFeatures.add(fin)
            root_done = True
            root_faces = set(f.tempId for f in root_fillet.faces)
        except:
            pass

        # --- soften: round the bigger edges, leaving the tab-root fillet alone. All at once
        #     first; failing that, one edge at a time so one bad edge can't sink the rest ---
        soft = [bracket.edges.item(i) for i in range(bracket.edges.count)
                if bracket.edges.item(i).length >= EDGE_MIN_LEN * MM
                and not any(f.tempId in root_faces for f in bracket.edges.item(i).faces)]
        skipped = 0
        coll = adsk.core.ObjectCollection.create()
        for e in soft:
            coll.add(e)
        fin = root.features.filletFeatures.createInput()
        fin.addConstantRadiusEdgeSet(coll, val(EDGE_FILLET), True)
        try:
            root.features.filletFeatures.add(fin)
        except:
            for tk in [e.entityToken for e in soft]:
                ents = design.findEntityByToken(tk)
                if not ents:
                    continue
                one = adsk.core.ObjectCollection.create()
                one.add(ents[0])
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(one, val(EDGE_FILLET), False)
                try:
                    root.features.filletFeatures.add(fin)
                except:
                    skipped += 1

        # --- move it into a straight piece's slot: turn it so the tab points +Y (into the
        #     frame), then slide it to the bracket's place along the piece ---
        moved = adsk.core.ObjectCollection.create()
        moved.add(bracket)
        mt = adsk.core.Matrix3D.create()
        mt.setToRotation(math.pi, adsk.core.Vector3D.create(0, 0, 1), adsk.core.Point3D.create(0, 0, 0))
        mt.translation = adsk.core.Vector3D.create(PITCH / 2.0 * MM, -INNER_OFF * MM, 0)
        mi = root.features.moveFeatures.createInput2(moved)
        mi.defineAsFreeMove(mt)
        root.features.moveFeatures.add(mi)

        ui.messageBox('Created Bracket_3way_tab (print 12, in place of the plain 3-way brackets).\n'
                      'Tab %.0f wide x %.0f thick, %.0f out from the flat edge; '
                      'screw hole %.1f dia at %.0f out (M3x8 button or pan head, from the back).\n'
                      'Back pocket: volume %.1f -> %.1f cm3 (%.0f%% less).\n'
                      'Tab root fillet %s; edge fillets skipped: %d.'
                      % (TAB_W, TAB_T, TAB_REACH, SCREW_CLEAR_D, SCREW_Y,
                         solid_vol, pocketed_vol, 100.0 * (1.0 - pocketed_vol / solid_vol),
                         'done' if root_done else 'FAILED', skipped))
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
