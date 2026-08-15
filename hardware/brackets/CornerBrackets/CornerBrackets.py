# CornerBrackets.py
# Fusion 360 script: wall-mount standoff bracket for the 3 corners of Triangel.
# Iteration 1 = flat on one wall; all three corners use this same part (print 3).
#
# "Z" standoff: a full-height back web, with the TILE PAD (M3 insert) at the top on
# one side and the WALL FOOT (single screw/nail hole) at the bottom on the other side.
# Offsetting the pad and foot to opposite sides leaves the wall screw drivable and
# keeps the insert clear -- and the full-height web is the stiff part.
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
TILE_T     = 8.0    # tile pad thickness (holds the M3x4x5 insert: 7mm pocket + ~1mm floor)
FOOT_X     = 30.0   # wall foot reach (X, opposite side from the pad)
FOOT_Y     = 30.0   # wall foot width (Y); centered on the web, so it grows past WIDTH symmetrically
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

        ins_x, ins_y = WEB_T + PAD_X / 2.0, WIDTH / 2.0   # insert position (unchanged)

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
            # rectangle from the web (x=0) out, but the far edge is the normal-bracket
            # border arc (radius PLATE_R about the vertex, which is R_HOLE toward -X of the insert)
            vx = ins_x - R_HOLE
            xa0 = vx + math.sqrt(PLATE_R ** 2 - (0.0 - ins_y) ** 2)      # arc meets y=0
            xa1 = vx + math.sqrt(PLATE_R ** 2 - (WIDTH - ins_y) ** 2)    # arc meets y=WIDTH
            xmid = vx + PLATE_R                                          # arc apex at y=ins_y
            sk = root.sketches.add(plane_at(z0))
            P = lambda x, y: adsk.core.Point3D.create(x * MM, y * MM, 0)
            crv = sk.sketchCurves
            crv.sketchLines.addByTwoPoints(P(0, 0), P(xa0, 0))
            crv.sketchArcs.addByThreePoints(P(xa0, 0), P(xmid, ins_y), P(xa1, WIDTH))
            crv.sketchLines.addByTwoPoints(P(xa1, WIDTH), P(0, WIDTH))
            crv.sketchLines.addByTwoPoints(P(0, WIDTH), P(0, 0))
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

        # --- solid: web spine + tile pad (top, +X, arc-clipped) + wall foot (bottom, -X) ---
        add_box(0, WEB_T, 0, WIDTH, 0, STANDOFF)                     # back web (full height)
        add_pad(STANDOFF - TILE_T, STANDOFF)                         # tile pad, outer edge = border arc
        add_box(-FOOT_X, WEB_T, WIDTH / 2.0 - FOOT_Y / 2.0, WIDTH / 2.0 + FOOT_Y / 2.0, 0, FOOT_T)  # wall foot (overlaps web, wider in Y)

        if GUSSET_H > 0:                                             # braces at both inside corners (before the fillets)
            add_gusset(0.0, -1, FOOT_T, +1)                         # web -> foot (base)
            add_gusset(WEB_T, +1, STANDOFF - TILE_T, -1)            # web -> pad (top, reversed)

        # --- soften edges BEFORE the holes: ROUND_R everywhere except the border arc
        #     (both the top and bottom of the clipped edge), which gets the gentler
        #     ARC_FILLET so that edge stays an even thickness top to bottom ---
        vx = ins_x - R_HOLE

        def is_border_arc(e):
            g = e.geometry
            cen = getattr(g, 'center', None)
            rad = getattr(g, 'radius', None)
            if cen is None or rad is None:
                return False
            return (abs(rad - PLATE_R * MM) < 0.2 * MM
                    and abs(cen.x - vx * MM) < 0.3 * MM
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
        wall_x, wall_y = -FOOT_X / 2.0, WIDTH / 2.0
        cut_cyl(wall_x, wall_y, FOOT_T, FOOT_T + 0.5, WALL_HOLE_D)            # wall hole through the foot
        if WASHER_DEPTH > 0:                                                  # counterbore for the washer / nail head
            cut_cyl(wall_x, wall_y, FOOT_T, WASHER_DEPTH, WASHER_D)

        ui.messageBox('Created CornerBracket (print 3).\n'
                      'Foot %.0fx%.0f + %.0fx%.0fmm gussets under foot AND pad; Ø%.0f wall hole + Ø%.0f counterbore.'
                      % (FOOT_X, FOOT_Y, GUSSET_L, GUSSET_H, WALL_HOLE_D, WASHER_D))
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
