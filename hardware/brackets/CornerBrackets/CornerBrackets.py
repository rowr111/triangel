# CornerBrackets.py
# Fusion 360 script: wall-mount standoff bracket for the 3 corners of Triangel.
# Iteration 1 = flat on one wall; all three corners use this same part (print 3).
#
# "Z" standoff: a full-height back web, with the TILE PAD (M3 insert) at the top and
# the WALL FOOT (single screw/nail hole) at the bottom. Both reach off the SAME side --
# see PAD_DIR. The full-height web is the stiff part. The wall screw stays reachable
# because the pad's far edge stops 1.6mm short of the wall hole, 48mm above it.
#
# The pad's outer edge is clipped to the SAME border arc as a normal tile bracket
# (radius PLATE_R about the vertex) so it never reaches past their footprint into the
# tile's back components; the rest of the pad stays rectangular.
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.
# All dimensions in mm; Fusion's API is cm internally, so we scale by MM=0.1.

import adsk.core, adsk.fusion, traceback, math

# ---- Editable parameters (mm) -------------------------------------------------
STANDOFF   = 60.0   # gap from wall to tile back (wall foot face -> tile pad face)
WIDTH      = 20.0   # bracket width (Y)
WEB_T      = 8.0    # back web / spine thickness (X)

PAD_X      = 16.0   # sets the insert position out past the web (X)
PAD_DIR    = -1     # which way the pad reaches off the web: -1 = back over the foot,
                    # +1 = out the other side, the way this bracket used to have it
PAD_TIP_W   = 11.0  # pad width at the back, where it meets the web. The tile has
                    # converged to 12.2mm there, so full WIDTH stands 3.9mm out each side
PAD_TAPER_L = 8.0   # how far out from that face the taper runs before it reaches WIDTH
PAD_TAPER_FADE = 30.0  # how far DOWN the web the narrowing fades back to full width.
                       # Cutting straight through the pad puts the web back in one step.
                       # 30 over a 4.5mm recovery is an 8.5 degree run-out
TILE_T     = 8.0    # tile pad thickness (holds the M3x4x5 insert: 7mm pocket + ~1mm floor)
WALL_HOLE_X = 15.0  # how far out from the web face the wall hole sits
FOOT_RIM    = 3.0   # material left past the washer recess at the foot's far end. FOOT_X
                    # is derived from it so the rim is even all round rather than long
                    # at the end - the sides are (FOOT_Y - WASHER_D)/2, the same 3.0
FOOT_Y     = WIDTH  # wall foot width (Y); centered on the web, so this sits it flush
FOOT_T     = 4.0    # wall foot thickness

GUSSET_H   = 16.0   # web-foot brace: rib height up the web (0 = no gusset)
GUSSET_L   = 6.0    # rib reach out along the foot (kept short to clear the wall hole/counterbore)
GUSSET_W   = WIDTH  # rib width in Y (across the web)

PLATE_R    = 34.0   # normal-bracket border radius -- the pad's outer edge stops here
R_HOLE     = 28.6   # hole-to-vertex distance (same as the tile brackets)

ROUND_R    = 3.0    # edge rounding; script tries this then backs off if a radius won't fit
ARC_FILLET = 0.5    # gentler round just on the top border-arc edge (it's right at the edge)

INSERT_HOLE_D = 3.6 # M3x4x5 insert (Ø3.6 hole) -- the longer 5mm insert for a sturdier hold
INSERT_DEPTH  = 7.0
SCREW_CLEAR_D = 3.4 # screw clearance below the insert pocket
MOUTH_FILLET  = 0.5 # lead-in round at the top of the insert pocket (tile-contact face)
WALL_HOLE_D   = 6.0 # single wall hole -- clears #6..#12 / M4-M6, washer spans it
WASHER_D      = 14.0 # counterbore on the foot's outer face for a washer / nail head (0 = none)
WASHER_DEPTH  = 1.5  # how deep to recess the washer / head
# ------------------------------------------------------------------------------

MM = 0.1

# Foot reach, derived so the washer recess sits in an even border: FOOT_RIM past its far
# edge, and (FOOT_Y - WASHER_D)/2 at each side. Set it directly to override.
FOOT_X = WALL_HOLE_X + WASHER_D / 2.0 + FOOT_RIM


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
                state['body'].name = 'CornerBracket'

        def add_box(x0, x1, y0, y1, z0, z1):
            sk = root.sketches.add(plane_at(z0))
            sk.sketchCurves.sketchLines.addTwoPointRectangle(
                adsk.core.Point3D.create(x0 * MM, y0 * MM, 0),
                adsk.core.Point3D.create(x1 * MM, y1 * MM, 0))
            extrude(sk, z0, z1)

        def add_pad(z0, z1):
            # rectangle from the web face out, but the far edge is the normal-bracket
            # border arc (radius PLATE_R about the vertex, which is R_HOLE back from the insert)
            vu = ins_u - R_HOLE
            ua0 = vu + math.sqrt(PLATE_R ** 2 - (0.0 - ins_y) ** 2)      # arc meets y=0
            ua1 = vu + math.sqrt(PLATE_R ** 2 - (WIDTH - ins_y) ** 2)    # arc meets y=WIDTH
            umid = vu + PLATE_R                                          # arc apex at y=ins_y
            sk = root.sketches.add(plane_at(z0))
            P = lambda x, y: adsk.core.Point3D.create(x * MM, y * MM, 0)
            crv = sk.sketchCurves
            crv.sketchLines.addByTwoPoints(P(PX(0), 0), P(PX(ua0), 0))
            crv.sketchArcs.addByThreePoints(P(PX(ua0), 0), P(PX(umid), ins_y), P(PX(ua1), WIDTH))
            crv.sketchLines.addByTwoPoints(P(PX(ua1), WIDTH), P(PX(0), WIDTH))
            crv.sketchLines.addByTwoPoints(P(PX(0), WIDTH), P(PX(0), 0))
            extrude(sk, z0, z1)

        def cut_cyl(cx, cy, z_top, depth, dia):
            sk = root.sketches.add(plane_at(z_top))
            sk.sketchCurves.sketchCircles.addByCenterRadius(
                adsk.core.Point3D.create(cx * MM, cy * MM, 0), (dia / 2.0) * MM)
            inp = exts.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.CutFeatureOperation)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(-depth * MM))  # cut downward (-Z)
            inp.participantBodies = [state['body']]
            exts.add(inp)

        def fillet_mouth(cx, cy, z, radius, fr):
            # find the circular rim at the pocket top (z, radius) and round it
            b = state['body']
            edges = adsk.core.ObjectCollection.create()
            for i in range(b.edges.count):
                e = b.edges.item(i)
                g = e.geometry
                if isinstance(g, adsk.core.Circle3D):
                    c = g.center
                    if (abs(c.z - z * MM) < 0.05 * MM and abs(g.radius - radius * MM) < 0.1 * MM
                            and abs(c.x - cx * MM) < 0.2 * MM and abs(c.y - cy * MM) < 0.2 * MM):
                        edges.add(e)
            if edges.count:
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(fr * MM), True)
                try:
                    root.features.filletFeatures.add(fin)
                except:
                    pass

        def add_gusset(x_web, x_dir, z_base, z_dir):
            # triangular rib bracing the web (face at x_web) to a surface (foot or pad at z_base);
            # x_dir = which way it reaches out (+/-X), z_dir = which way it climbs the web (+/-Z).
            # Centered on the web width and kept short so it clears the holes.
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.xZConstructionPlane, adsk.core.ValueInput.createByReal(WIDTH / 2.0 * MM))
            pl = root.constructionPlanes.add(pin)
            if pl.geometry.origin.y < 0:                      # offset went -Y; put the plane on the +Y side
                pin = root.constructionPlanes.createInput()
                pin.setByOffset(root.xZConstructionPlane, adsk.core.ValueInput.createByReal(-WIDTH / 2.0 * MM))
                pl = root.constructionPlanes.add(pin)
            py = pl.geometry.origin.y                         # actual plane Y (cm)
            sk = root.sketches.add(pl)
            ovl = 0.6                                         # dip into web/surface so it merges cleanly
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

        def cut_pad_taper():
            # Narrow the pad's back, where it meets the web: the tile has converged toward
            # its tip by then, so a full-WIDTH slab stands out past it either side. The wedge
            # is LOFTED, full at the tile face and gone PAD_TAPER_FADE further down, so the
            # web comes back to width gradually. It has to reach past the pad into the web at
            # all, because over this stretch the web reaches the tile face too.
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
                    ln.addByTwoPoints(
                        adsk.core.Point3D.create(pts[i][0] * MM, pts[i][1] * MM, 0),
                        adsk.core.Point3D.create(pts[(i + 1) % len(pts)][0] * MM,
                                                 pts[(i + 1) % len(pts)][1] * MM, 0))
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
                lin.participantBodies = [state['body']]
                root.features.loftFeatures.add(lin)

        # --- solid: web spine + tile pad (top, arc-clipped) + wall foot (bottom) ---
        add_box(0, WEB_T, 0, WIDTH, 0, STANDOFF)                     # back web (full height)
        add_pad(STANDOFF - TILE_T, STANDOFF)                         # tile pad, outer edge = border arc
        add_box(-FOOT_X, WEB_T, WIDTH / 2.0 - FOOT_Y / 2.0, WIDTH / 2.0 + FOOT_Y / 2.0, 0, FOOT_T)  # wall foot (overlaps web, wider in Y)

        if GUSSET_H > 0:                                             # braces at both inside corners (before the fillets)
            add_gusset(0.0, -1, FOOT_T, +1)                         # web -> foot (base)
            add_gusset(PX(WEB_T), PAD_DIR, STANDOFF - TILE_T, -1)   # web -> pad (top, reversed)

        # After the gussets, so the taper shapes the top one's near corner too rather than
        # leaving it standing full width in the stretch the cut just narrowed.
        cut_pad_taper()

        # --- soften edges BEFORE the holes: ROUND_R everywhere except the border arc
        #     (both the top and bottom of the clipped edge), which gets the gentler
        #     ARC_FILLET so that edge stays an even thickness top to bottom ---
        def is_border_arc(e):
            g = e.geometry
            cen = getattr(g, 'center', None)
            rad = getattr(g, 'radius', None)
            if cen is None or rad is None:
                return False
            return (abs(rad - PLATE_R * MM) < 0.2 * MM
                    and abs(cen.x - arc_cx * MM) < 0.3 * MM
                    and abs(cen.y - ins_y * MM) < 0.3 * MM)

        body = state['body']
        # The foot is only FOOT_T thick, so its edges can't take the full ROUND_R: the
        # top and bottom rounds would meet in the middle and eat the whole plate. Cap the
        # foot's edges well under half its thickness; everything taller keeps the big round.
        foot_r = min(ROUND_R, FOOT_T / 2.0 - 0.2)
        # Round every structural edge, ONE fillet feature per edge (found by token so it
        # survives earlier fillets); each takes the biggest radius that computes.
        big, arc_tokens = [], []
        for i in range(body.edges.count):
            e = body.edges.item(i)
            if is_border_arc(e):
                arc_tokens.append(e.entityToken)
                continue
            thin = e.boundingBox.maxPoint.z <= (FOOT_T + 0.05) * MM     # edge lives in the foot plate
            big.append((e.entityToken, foot_r if thin else ROUND_R))
        for tk, r0 in big:
            ents = design.findEntityByToken(tk)
            if not ents:
                continue
            for rr in [r0, r0 * 0.66, r0 * 0.5, r0 * 0.33]:
                coll = adsk.core.ObjectCollection.create()
                coll.add(ents[0])
                fin = root.features.filletFeatures.createInput()
                fin.addConstantRadiusEdgeSet(coll, adsk.core.ValueInput.createByReal(rr * MM), False)
                try:
                    root.features.filletFeatures.add(fin)
                    break
                except:
                    continue
        for tk in arc_tokens:                                        # gentler round on the border arc
            ents = design.findEntityByToken(tk)
            if not ents:
                continue
            coll = adsk.core.ObjectCollection.create()
            coll.add(ents[0])
            fin = root.features.filletFeatures.createInput()
            fin.addConstantRadiusEdgeSet(coll, adsk.core.ValueInput.createByReal(ARC_FILLET * MM), False)
            try:
                root.features.filletFeatures.add(fin)
            except:
                pass

        # --- holes ---
        cut_cyl(ins_x, ins_y, STANDOFF, INSERT_DEPTH, INSERT_HOLE_D)          # insert pocket, opens on the tile face
        cut_cyl(ins_x, ins_y, STANDOFF, TILE_T + 0.5, SCREW_CLEAR_D)          # screw clearance through the pad
        fillet_mouth(ins_x, ins_y, STANDOFF, INSERT_HOLE_D / 2.0, MOUTH_FILLET)  # 0.5mm lead-in round at the pocket mouth
        wall_x, wall_y = -WALL_HOLE_X, WIDTH / 2.0
        cut_cyl(wall_x, wall_y, FOOT_T, FOOT_T + 0.5, WALL_HOLE_D)            # wall hole through the foot
        if WASHER_DEPTH > 0:                                                  # counterbore for the washer / nail head
            cut_cyl(wall_x, wall_y, FOOT_T, WASHER_DEPTH, WASHER_D)

        ui.messageBox('Created CornerBracket (print 3).\n'
                      'Foot %.0fx%.0f, flush with the %.0fmm web; gussets %.0fx%.0f under foot AND pad.\n'
                      'Pad narrows to %.0f at the web, full %.0f from %.0fmm out.\n'
                      'Wall hole %.0f dia + %.0f dia counterbore, %.1fmm of rim each side.'
                      % (FOOT_X, FOOT_Y, WIDTH, GUSSET_L, GUSSET_H,
                         PAD_TIP_W, WIDTH, PAD_TAPER_L,
                         WALL_HOLE_D, WASHER_D, (FOOT_Y - WASHER_D) / 2.0))
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
