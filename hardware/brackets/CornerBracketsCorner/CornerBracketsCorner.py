# CornerBracketsCorner.py  -- v1 (room-corner mount)
#
# Built on the working flat CornerBrackets.py: the 90-degree standoff (web) is identical.
# The wall FOOT is built flat and then tilted OUT about a hinge by the corner angle so it
# lies against an angled wall. The tile PAD reaches back OVER the feet rather than away
# from them -- see PAD_DIR. The flat bracket points it the other way to keep the wall screw
# drivable, which stops being the right trade once the feet fold into a corner.
#
# An equilateral triangle capping a right-angle corner sits at the "magic angle"
# arctan(sqrt2) = 54.7deg to each wall, so the foot tilts 54.7deg off flat and the
# web->foot fold opens to ~144.7deg. Single foot for now; we split it into two 90deg
# panels for the actual corner once the angle is confirmed.
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.  Dims mm; API is cm (scale MM).

import adsk.core, adsk.fusion, traceback, math

# ---- Editable parameters (mm / deg) -------------------------------------------
STANDOFF   = 50.0   # pad-to-foot standoff (perpendicular, same as flat bracket)
WIDTH      = 20.0
WEB_T      = 6.0

PAD_X      = 20.0
PAD_DIR    = -1     # which way the pad reaches off the web: -1 = back over the feet,
                    # +1 = out into the room, the way the flat bracket has it
PAD_TIP_W   = 11.0  # pad width at the back, where it meets the web. The tile has converged
                    # to 12.2mm there, so the full WIDTH stands ~3.9mm proud on each side
PAD_TAPER_L = 8.0   # how far out from that face the taper runs before it reaches WIDTH
PAD_TAPER_FADE = 30.0  # how far DOWN the web the narrowing fades back to full width. Cutting
                       # straight through the pad instead puts the web back in a single step.
                       # 30 over a 4.5mm recovery is an 8.5 degree run-out
TILE_T     = 8.0
FOOT_X     = 30.0
FOOT_T     = 4.0

PLATE_R    = 34.0   # pad border-arc radius
R_HOLE     = 28.6

INSERT_HOLE_D = 3.6 # M3x4x5 insert
INSERT_DEPTH  = 7.0
SCREW_CLEAR_D = 3.0
WALL_HOLE_D   = 6.0
WASHER_D      = 14.0 # counterbore on each foot's outer face for a washer / nail head (0 = none)
WASHER_DEPTH  = 1.5  # how deep to recess the washer / head

FOLD_TILT  = 54.7   # deg to tilt the foot OUT from flat (arctan(sqrt2) = the corner angle)
FOOT_DX    = 11.0     # scootch the foot horizontally (+ = toward the standoff) to line its corner up
FOOT_DZ    = -8.8   # scootch the foot vertically  (- = down) to line its corner up
FOOT_SPLAY = 45.0   # deg each foot folds about the spine into the corner V (tune for a 90deg corner)
FOOT_WIDTH = 40.0   # width of EACH foot panel out from the spine (was WIDTH/2=10; wider = more wall contact)
FOOT_HOLE_OFFSET = 6.0  # shift each foot's hole toward its OUTER edge, off-center (0 = centered)
STANDOFF_DROP = 12.0  # extend the standoff body DOWN past z=0 to reach + merge with the lowered feet
TRIM_FRONT_X  = 6.0   # trim the feet flush at this x (= standoff front face WEB_T; they poke ~5.5mm past it)
TRIM_TOP_Z    = 0.0   # stop the back-of-foot trim at this z so it doesn't run up into the pad/bracket
GUSSET_H   = 16.0   # under-pad brace: rib height down the web (0 = no gusset)
GUSSET_L   = 6.0    # rib reach out under the pad (kept short to clear the insert hole)
GUSSET_W   = WIDTH  # rib width in Y (across the pad)
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
        # The pad is laid out in u: distance out from whichever web face it springs from.
        # PX mirrors u about the web's midplane, so flipping PAD_DIR keeps the pad's overlap
        # with the web, and the part's thickness, exactly as they are.
        def PX(u):
            return u if PAD_DIR > 0 else WEB_T - u

        ins_u = WEB_T + PAD_X / 2.0
        ins_x, ins_y = PX(ins_u), WIDTH / 2.0
        arc_cx = PX(ins_u - R_HOLE)      # border-arc center, mirrored along with the pad

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
            vu = ins_u - R_HOLE
            ua0 = vu + math.sqrt(PLATE_R ** 2 - (0.0 - ins_y) ** 2)
            ua1 = vu + math.sqrt(PLATE_R ** 2 - (WIDTH - ins_y) ** 2)
            umid = vu + PLATE_R
            sk = root.sketches.add(plane_at(z0))
            crv = sk.sketchCurves
            crv.sketchLines.addByTwoPoints(P(PX(0), 0), P(PX(ua0), 0))
            crv.sketchArcs.addByThreePoints(P(PX(ua0), 0), P(PX(umid), ins_y), P(PX(ua1), WIDTH))
            crv.sketchLines.addByTwoPoints(P(PX(ua1), WIDTH), P(PX(0), WIDTH))
            crv.sketchLines.addByTwoPoints(P(PX(0), WIDTH), P(PX(0), 0))
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

        def cut_pad_taper():
            # Narrow the pad's back, where it meets the web: the tile has converged toward its
            # tip by then, so a full-WIDTH slab stands proud of it either side. The wedge is
            # LOFTED, full at the tile face and gone PAD_TAPER_FADE further down, so the web
            # comes back to width gradually. It has to reach past the pad into the web at all,
            # because over this stretch the web reaches the tile face too.
            m = (WIDTH / 2.0 - PAD_TIP_W / 2.0) / PAD_TAPER_L   # half-width gained per mm out
            u0 = -1.0                                           # start clear of the pad's back face
            out = WIDTH / 2.0 + 20.0                            # well outside the part

            def quad(z, side, y_near, y_far):
                sk = root.sketches.add(plane_at(z))
                ln = sk.sketchCurves.sketchLines
                pts = [(PX(u0), ins_y + side * y_near),
                       (PX(PAD_TAPER_L), ins_y + side * y_far),
                       (PX(PAD_TAPER_L), ins_y + side * out),
                       (PX(u0), ins_y + side * out)]
                for i in range(len(pts)):
                    ln.addByTwoPoints(P(*pts[i]), P(*pts[(i + 1) % len(pts)]))
                return sk.profiles.item(0)

            for side in (-1.0, 1.0):
                lin = root.features.loftFeatures.createInput(
                    adsk.fusion.FeatureOperations.CutFeatureOperation)
                # top section follows the taper line; the bottom one clears the web
                # entirely, so the cut runs out to nothing between them
                lin.loftSections.add(quad(STANDOFF, side, PAD_TIP_W / 2.0 + m * u0, WIDTH / 2.0))
                lin.loftSections.add(quad(STANDOFF - PAD_TAPER_FADE, side,
                                          WIDTH / 2.0 + 0.5, WIDTH / 2.0 + 0.5))
                lin.isSolid = True
                lin.participantBodies = [main]
                root.features.loftFeatures.add(lin)

        # --- 1) pad + web (the 90-degree standoff); the web is the flat bracket's ---
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
            if WASHER_DEPTH > 0:                                        # counterbore on the outer face for a washer / head
                cut_cyl(-FOOT_X / 2.0, hole_y, FOOT_T, WASHER_DEPTH, WASHER_D, foot)

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

        def bb6(b):                                                       # bbox as plain cm numbers
            bb = b.boundingBox
            return (bb.minPoint.x, bb.minPoint.y, bb.minPoint.z,
                    bb.maxPoint.x, bb.maxPoint.y, bb.maxPoint.z)
        foot_bbs = [bb6(foot1), bb6(foot2)]   # capture the foot volumes before the combine consumes them

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

        # --- 5b) one brace under the pad (web -> pad), same rib as the flat corner bracket.
        #        Added AFTER the foot trims so the trim slab doesn't chop it. ---
        def add_gusset(x_web, x_dir, z_base, z_dir):
            # triangular rib bracing the web (face at x_web) to the pad underside (z_base);
            # x_dir = which way it reaches out, z_dir = which way it climbs the web.
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.xZConstructionPlane, adsk.core.ValueInput.createByReal(WIDTH / 2.0 * MM))
            pl = root.constructionPlanes.add(pin)
            if pl.geometry.origin.y < 0:                      # offset went -Y; put the plane on the +Y side
                pin = root.constructionPlanes.createInput()
                pin.setByOffset(root.xZConstructionPlane, adsk.core.ValueInput.createByReal(-WIDTH / 2.0 * MM))
                pl = root.constructionPlanes.add(pin)
            py = pl.geometry.origin.y                         # actual plane Y (cm)
            sk = root.sketches.add(pl)
            ovl = 0.6                                         # dip into web/pad so it merges cleanly
            def SP(x, z):                                     # a model point on the plane -> sketch space
                return sk.modelToSketchSpace(adsk.core.Point3D.create(x * MM, py, z * MM))
            xa, zb = x_web - x_dir * ovl, z_base - z_dir * ovl
            ln = sk.sketchCurves.sketchLines
            ln.addByTwoPoints(SP(xa, zb), SP(xa, z_base + z_dir * GUSSET_H))
            ln.addByTwoPoints(SP(xa, z_base + z_dir * GUSSET_H), SP(x_web + x_dir * GUSSET_L, zb))
            ln.addByTwoPoints(SP(x_web + x_dir * GUSSET_L, zb), SP(xa, zb))
            inp = exts.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.JoinFeatureOperation)
            inp.setSymmetricExtent(adsk.core.ValueInput.createByReal(GUSSET_W * MM), True)
            inp.participantBodies = [state['body']]
            exts.add(inp)

        if GUSSET_H > 0:
            add_gusset(PX(WEB_T), PAD_DIR, STANDOFF - TILE_T, -1)   # web -> pad (under the pad)

        # After the gusset, so the taper shapes its near corner too rather than leaving the
        # gusset standing full width in the stretch the cut just narrowed.
        cut_pad_taper()

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
                    and abs(cen.x - arc_cx * MM) < 0.3 * MM
                    and abs(cen.y - ins_y * MM) < 0.3 * MM)

        # The feet are only FOOT_T thick, so their edges can't take the full ROUND_R (the top +
        # bottom rounds would meet and eat the panel). Cap any edge that lives inside a foot
        # volume to well under half that thickness; everything else keeps the big round.
        foot_r = min(ROUND_R, FOOT_T / 2.0 - 0.2)

        def in_feet(e):
            bb = e.boundingBox
            cx = (bb.minPoint.x + bb.maxPoint.x) / 2.0
            cy = (bb.minPoint.y + bb.maxPoint.y) / 2.0
            cz = (bb.minPoint.z + bb.maxPoint.z) / 2.0
            m = 0.5 * MM
            for (x0, y0, z0, x1, y1, z1) in foot_bbs:
                if x0-m <= cx <= x1+m and y0-m <= cy <= y1+m and z0-m <= cz <= z1+m:
                    return True
            return False

        # big fillet -- ONE feature per edge (found by entity token so it survives the earlier
        # fillets); each edge takes the biggest radius that computes, fussy ones just get skipped
        tokens = []
        for i in range(main.edges.count):
            e = main.edges.item(i)
            if is_circle(e) or is_border_arc(e):                 # border arc gets 0.5mm instead
                continue
            tokens.append((e.entityToken, foot_r if in_feet(e) else ROUND_R))
        for tk, r0 in tokens:
            ents = design.findEntityByToken(tk)
            if not ents:
                continue
            for rr in [r0, r0 * 0.66, r0 * 0.5, r0 * 0.33]:
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
