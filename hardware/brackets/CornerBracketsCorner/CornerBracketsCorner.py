# CornerBracketsCorner.py  -- v1 (room-corner mount)
#
# Built on the working flat CornerBrackets.py: the tile PAD + 90-degree standoff (web)
# are identical; the ONLY change is the wall FOOT, which is built flat and then tilted
# OUT about a hinge by the corner angle so it lies against an angled wall.
#
# An equilateral triangle capping a right-angle corner sits at the "magic angle"
# arctan(sqrt2) = 54.7deg to each wall, so the foot tilts 54.7deg off flat and the
# web->foot fold opens to ~144.7deg. Single foot for now; we split it into two 90deg
# panels for the actual corner once the angle is confirmed.
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.  Dims mm; API is cm (scale MM).

import adsk.core, adsk.fusion, traceback, math

# ---- Editable parameters (mm / deg) -------------------------------------------
STANDOFF   = 30.0   # pad-to-foot standoff (perpendicular, same as flat bracket)
WIDTH      = 20.0
WEB_T      = 6.0

PAD_X      = 20.0
TILE_T     = 8.0
FOOT_X     = 20.0
FOOT_T     = 6.0

PLATE_R    = 34.0   # pad border-arc radius
R_HOLE     = 28.6

INSERT_HOLE_D = 3.6 # M3x4x5 insert
INSERT_DEPTH  = 7.0
SCREW_CLEAR_D = 3.0
WALL_HOLE_D   = 5.0

FOLD_TILT  = 54.7   # deg to tilt the foot OUT from flat (arctan(sqrt2) = the corner angle)
FOOT_DX    = 11.0     # scootch the foot horizontally (+ = toward the standoff) to line its corner up
FOOT_DZ    = -8.8   # scootch the foot vertically  (- = down) to line its corner up
FOOT_SPLAY = 45.0   # deg each foot folds about the spine into the corner V (tune for a 90deg corner)
FOOT_WIDTH = 30.0   # width of EACH foot panel out from the spine (was WIDTH/2=10; wider = more wall contact)
FOOT_HOLE_OFFSET = 6.0  # shift each foot's hole toward its OUTER edge, off-center (0 = centered)
STANDOFF_DROP = 12.0  # extend the standoff body DOWN past z=0 to reach + merge with the lowered feet
TRIM_FRONT_X  = 6.0   # trim the feet flush at this x (= standoff front face WEB_T; they poke ~5.5mm past it)
TRIM_TOP_Z    = 0.0   # stop the back-of-foot trim at this z so it doesn't run up into the pad/bracket
ROUND_R       = 3.0   # big fillet on all non-hole edges (backs off if a radius won't fit)
# ------------------------------------------------------------------------------

MM = 0.1


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
        state = {'body': None}
        ins_x, ins_y = WEB_T + PAD_X / 2.0, WIDTH / 2.0

        def plane_at(z):
            if abs(z) < 1e-9:
                return root.xYConstructionPlane
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * MM))
            return root.constructionPlanes.add(pin)

        def P(x, y):
            return adsk.core.Point3D.create(x * MM, y * MM, 0)

        def op():
            return (adsk.fusion.FeatureOperations.NewBodyFeatureOperation if state['body'] is None
                    else adsk.fusion.FeatureOperations.JoinFeatureOperation)

        def extrude(sk, z0, z1):
            inp = exts.createInput(sk.profiles.item(0), op())
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal((z1 - z0) * MM))
            if state['body'] is not None:
                inp.participantBodies = [state['body']]
            f = exts.add(inp)
            if state['body'] is None:
                state['body'] = f.bodies.item(0)
                state['body'].name = 'CornerBracketCorner'

        def add_box(x0, x1, y0, y1, z0, z1):
            sk = root.sketches.add(plane_at(z0))
            sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0, y0), P(x1, y1))
            extrude(sk, z0, z1)

        def add_pad(z0, z1):
            vx = ins_x - R_HOLE
            xa0 = vx + math.sqrt(PLATE_R ** 2 - (0.0 - ins_y) ** 2)
            xa1 = vx + math.sqrt(PLATE_R ** 2 - (WIDTH - ins_y) ** 2)
            xmid = vx + PLATE_R
            sk = root.sketches.add(plane_at(z0))
            crv = sk.sketchCurves
            crv.sketchLines.addByTwoPoints(P(0, 0), P(xa0, 0))
            crv.sketchArcs.addByThreePoints(P(xa0, 0), P(xmid, ins_y), P(xa1, WIDTH))
            crv.sketchLines.addByTwoPoints(P(xa1, WIDTH), P(0, WIDTH))
            crv.sketchLines.addByTwoPoints(P(0, WIDTH), P(0, 0))
            extrude(sk, z0, z1)

        def cut_cyl(cx, cy, z_top, depth, dia, body):
            sk = root.sketches.add(plane_at(z_top))
            sk.sketchCurves.sketchCircles.addByCenterRadius(P(cx, cy), (dia / 2.0) * MM)
            inp = exts.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.CutFeatureOperation)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(-depth * MM))
            inp.participantBodies = [body]
            exts.add(inp)

        def cut_slab(x0, x1, y0, y1, z_top, depth, body):
            # remove the rectangular X-Y region x[x0,x1] y[y0,y1] from z_top downward by depth
            sk = root.sketches.add(plane_at(z_top))
            sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0, y0), P(x1, y1))
            inp = exts.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.CutFeatureOperation)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(-depth * MM))
            inp.participantBodies = [body]
            exts.add(inp)

        # --- 1) pad + web (the 90-degree standoff) -- same as the flat bracket ---
        add_box(0, WEB_T, 0, WIDTH, -STANDOFF_DROP, STANDOFF)     # back web (extended down to meet the feet)
        add_pad(STANDOFF - TILE_T, STANDOFF)                      # arc-clipped tile pad
        main = state['body']
        cut_cyl(ins_x, ins_y, STANDOFF, INSERT_DEPTH, INSERT_HOLE_D, main)   # insert pocket
        cut_cyl(ins_x, ins_y, STANDOFF, TILE_T + 0.5, SCREW_CLEAR_D, main)   # screw clearance

        # --- 2) build the foot TWICE: each tilted out, scootched, then splayed to
        #        one wall of the corner (splay_sign = +1 / -1 for the two panels) ---
        mf = root.features.moveFeatures

        def move_body(body, define):
            c = adsk.core.ObjectCollection.create()
            c.add(body)
            inp = mf.createInput2(c)
            define(inp)
            mf.add(inp)

        def build_foot(y0, y1, splay_sign):
            skf = root.sketches.add(root.xYConstructionPlane)
            skf.sketchCurves.sketchLines.addTwoPointRectangle(P(-FOOT_X, y0), P(WEB_T, y1))
            fin = exts.createInput(skf.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
            fin.setDistanceExtent(False, adsk.core.ValueInput.createByReal(FOOT_T * MM))
            foot = exts.add(fin).bodies.item(0)
            foot.name = 'foot'
            hole_y = (y0 + y1) / 2.0 + splay_sign * FOOT_HOLE_OFFSET   # off-center, toward the outer edge
            cut_cyl(-FOOT_X / 2.0, hole_y, FOOT_T, FOOT_T + 0.5, WALL_HOLE_D, foot)

            # 1) tilt the flat half OUT about the Y hinge (same lean as the single foot)
            move_body(foot, lambda inp: inp.defineAsRotate(
                root.yConstructionAxis, adsk.core.ValueInput.createByReal(math.radians(-FOLD_TILT))))
            # 2) FOLD it about the TILTED foot's own centerline (its real 3D spine) into the V.
            #    That spine is the X axis rotated by the tilt: (cos, 0, sin) through y=WIDTH/2.
            fx = math.cos(math.radians(FOLD_TILT))
            fz = math.sin(math.radians(FOLD_TILT))
            rf = adsk.core.Matrix3D.create()
            rf.setToRotation(math.radians(splay_sign * FOOT_SPLAY),
                             adsk.core.Vector3D.create(fx, 0.0, fz), P(0, WIDTH / 2.0))
            move_body(foot, lambda inp: inp.defineAsFreeMove(rf))
            # 3) scootch over + down so the corner lines up with the standoff
            tm = adsk.core.Matrix3D.create()
            tm.translation = adsk.core.Vector3D.create(FOOT_DX * MM, 0.0, FOOT_DZ * MM)
            move_body(foot, lambda inp: inp.defineAsFreeMove(tm))
            return foot

        foot1 = build_foot(WIDTH / 2.0, WIDTH / 2.0 + FOOT_WIDTH, 1.0)    # panel out toward +Y
        foot2 = build_foot(WIDTH / 2.0 - FOOT_WIDTH, WIDTH / 2.0, -1.0)   # panel out toward -Y

        # --- 3) join both feet onto the pad + standoff ---
        tools = adsk.core.ObjectCollection.create()
        tools.add(foot1)
        tools.add(foot2)
        comb = root.features.combineFeatures.createInput(main, tools)
        comb.operation = adsk.fusion.FeatureOperations.JoinFeatureOperation
        root.features.combineFeatures.add(comb)

        # --- 4) trim the feet flush with the standoff's front face (they poke ~5.5mm past it) ---
        cut_slab(TRIM_FRONT_X, TRIM_FRONT_X + 80.0, -60.0, 80.0,
                 STANDOFF - TILE_T, STANDOFF - TILE_T + STANDOFF_DROP + 60.0, main)

        # --- 5) trim the standoff flush with the BACK of each foot (Jeanie's idea): build a slab
        #        on the wall side of a flat foot, run it through the SAME foot transforms so it lands
        #        on that foot's back plane, then subtract it -- shaves the corners poking past ---
        def apply_foot_transform(body, sgn):
            move_body(body, lambda inp: inp.defineAsRotate(
                root.yConstructionAxis, adsk.core.ValueInput.createByReal(math.radians(-FOLD_TILT))))
            fx, fz = math.cos(math.radians(FOLD_TILT)), math.sin(math.radians(FOLD_TILT))
            rf = adsk.core.Matrix3D.create()
            rf.setToRotation(math.radians(sgn * FOOT_SPLAY),
                             adsk.core.Vector3D.create(fx, 0.0, fz), P(0, WIDTH / 2.0))
            move_body(body, lambda inp: inp.defineAsFreeMove(rf))
            tm = adsk.core.Matrix3D.create()
            tm.translation = adsk.core.Vector3D.create(FOOT_DX * MM, 0.0, FOOT_DZ * MM)
            move_body(body, lambda inp: inp.defineAsFreeMove(tm))

        def trim_by_foot_back(sgn):
            sk = root.sketches.add(root.xYConstructionPlane)          # slab is z[-70,0] = wall side of the z=0 face
            sk.sketchCurves.sketchLines.addTwoPointRectangle(P(-80, -80), P(80, 80))
            fin = exts.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
            fin.setDistanceExtent(False, adsk.core.ValueInput.createByReal(-70 * MM))
            slab = exts.add(fin).bodies.item(0)
            slab.name = 'trimslab'
            apply_foot_transform(slab, sgn)
            # cap the slab: remove everything above TRIM_TOP_Z so the trim stops before the pad
            skc = root.sketches.add(plane_at(TRIM_TOP_Z))
            skc.sketchCurves.sketchLines.addTwoPointRectangle(P(-140, -140), P(140, 140))
            ic = exts.createInput(skc.profiles.item(0), adsk.fusion.FeatureOperations.CutFeatureOperation)
            ic.setDistanceExtent(False, adsk.core.ValueInput.createByReal(120 * MM))   # cut UPWARD (+z)
            ic.participantBodies = [slab]
            exts.add(ic)
            tools = adsk.core.ObjectCollection.create()
            tools.add(slab)
            cb = root.features.combineFeatures.createInput(main, tools)
            cb.operation = adsk.fusion.FeatureOperations.CutFeatureOperation
            root.features.combineFeatures.add(cb)

        trim_by_foot_back(1.0)
        trim_by_foot_back(-1.0)

        # --- 6) fillets: big round on all structural (non-hole) edges; 0.5mm on the hole rims ---
        fills = root.features.filletFeatures

        def is_circle(e):
            return isinstance(e.geometry, adsk.core.Circle3D)

        def is_border_arc(e):   # the pad's tile-contact border arc (radius PLATE_R about the vertex)
            g = e.geometry
            cen, rad = getattr(g, 'center', None), getattr(g, 'radius', None)
            if cen is None or rad is None:
                return False
            return (abs(rad - PLATE_R * MM) < 0.2 * MM
                    and abs(cen.x - (ins_x - R_HOLE) * MM) < 0.3 * MM
                    and abs(cen.y - ins_y * MM) < 0.3 * MM)

        # big fillet -- ONE feature per edge (found by entity token so it survives the earlier
        # fillets); each edge takes the biggest radius that computes, fussy ones just get skipped
        tokens = []
        for i in range(main.edges.count):
            e = main.edges.item(i)
            if not is_circle(e) and not is_border_arc(e):        # border arc gets 0.5mm instead
                tokens.append(e.entityToken)
        for tk in tokens:
            ents = design.findEntityByToken(tk)
            if not ents:
                continue
            for rr in [ROUND_R, 2.0, 1.0, 0.5]:
                coll = adsk.core.ObjectCollection.create()
                coll.add(ents[0])
                fin = fills.createInput()
                # tangent chain OFF: fillet only this edge so it can't reach into the excluded arc
                fin.addConstantRadiusEdgeSet(coll, adsk.core.ValueInput.createByReal(rr * MM), False)
                try:
                    fills.add(fin)
                    break
                except:
                    continue

        rim_tokens = []                                          # 0.5mm: pad border edge + hole rims
        arc_zs = []
        for i in range(main.edges.count):
            e = main.edges.item(i)
            keep = False
            if is_border_arc(e):
                keep = True                                       # front pad face top+bottom edges
                arc_zs.append(e.geometry.center.z / MM)
            elif is_circle(e):
                r = e.geometry.radius
                insert_top = (abs(r - INSERT_HOLE_D / 2.0 * MM) < 0.1 * MM
                              and abs(e.geometry.center.z - STANDOFF * MM) < 0.1 * MM)   # insert pocket mouth only
                foot_hole = abs(r - WALL_HOLE_D / 2.0 * MM) < 0.1 * MM                    # both rims of each foot hole
                keep = insert_top or foot_hole
            if keep:
                rim_tokens.append(e.entityToken)
        for tk in rim_tokens:                                     # one feature each so none can block the others
            ents = design.findEntityByToken(tk)
            if not ents:
                continue
            coll = adsk.core.ObjectCollection.create()
            coll.add(ents[0])
            fin = fills.createInput()
            fin.addConstantRadiusEdgeSet(coll, adsk.core.ValueInput.createByReal(0.5 * MM), True)
            try:
                fills.add(fin)
            except:
                pass

        ui.messageBox('Created CornerBracketCorner v5.\n'
                      'Big %.1fmm fillets (tangent chain off); 0.5mm on the front-face arc + '
                      'insert mouth + foot holes.\n'
                      'DIAG: arc edges at 0.5mm = %d (z = %s)   [want z=%.0f and %.0f]'
                      % (ROUND_R, len(arc_zs),
                         ', '.join('%.0f' % z for z in arc_zs) if arc_zs else 'none',
                         STANDOFF - TILE_T, STANDOFF))
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
