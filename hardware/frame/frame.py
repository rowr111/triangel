# frame.py
# Fusion 360 script: printed frame pieces that Triangel sits inside.
#
# Pieces join with a triangle joint that drops in from the front, so the triangle
# outline shows on the front face. The socket has a floor, so the tree seats at a fixed
# depth. Both outlines get a small fillet on the front edges, so the seam reads as a groove.
#
# Builds a STRAIGHT PIECE: picture-frame profile, tree on one end, socket on the other,
# and a slot in the middle for the tab on an edge bracket. The slot is open at the back;
# an M3 screw goes in from the back through the tab into an insert in the slot's ceiling.
#
# Also builds a CORNER PIECE: two arms meeting in a sharp 60 degree miter, tree on one
# arm and socket on the other, so all three corners are the same part. A fake joint is
# grooved in at the miter to match the real seams.
#
# Axes: X along the frame, Y across its front face, Z front-to-back (Z=0 is the back).
#
# Run from Utilities > Add-Ins > Scripts and Add-Ins.
# All dimensions in mm; Fusion's API is cm internally, so we scale by MM=0.1.

import adsk.core, adsk.fusion, traceback, math

# ---- Editable parameters (mm) -------------------------------------------------
FLOOR_T    = 4.0    # socket floor under the tree

TRUNK_W    = 6.0    # trunk width, where the tree leaves its piece
TRUNK_L    = 2.5    # trunk length before the triangle
TRI_SIDE   = 10.0   # side of the equilateral triangle

CLEAR      = 0.25   # gap between tree and socket, per side (room for the adhesive)
JOINT_R    = 0.5    # fillet on the front edges of the tree and the socket

# Straight piece profile: Y=0 is the inner face, next to the tiles
PITCH      = 103.46 # edge bracket spacing along a side: 100mm tile + 2 x 2mm gap x cos 30
BRACKET_T  = 5.0    # edge bracket plate; the frame back is flush with the bracket back
TILE_T     = 1.6    # tile board thickness
RISE       = 8.0    # how far the flat sits in front of the tile fronts
FRAME_R    = 4.0    # fillet on both top edges of the profile
FLAT_W     = 16.0   # flat strip along the top, where the joint sits
WALL       = 2.5    # wall left when the back is hollowed out (to save material)
END_SOLID  = 3.0    # solid length at the tree end, for the trunk to grow from

# Slot for the edge bracket tab, in the middle of the straight piece
TAB_W      = 10.0   # tab width (X)
TAB_T      = 3.0    # tab thickness, flush with the bracket back
SLOT_REACH = 13.0   # how far the slot runs in from the inner face (Y)
INSERT_Y   = 8.0    # insert center, measured from the inner face
INSERT_HOLE_D = 3.6 # JLC3DP M3x4x3 insert, same as the tile brackets
INSERT_DEPTH  = 5.0 # JLC requires a 5.0mm-deep hole for that insert

# Corner piece
INNER_OFF  = 1.0    # frame inner face, out from the line through the bracket centers
                    # (the tile edge is 1mm inside that line, then the 2mm gap)
FAKE_W     = 0.5    # fake joint groove width (a 0.25mm slit won't print cleanly)
FAKE_D     = 1.0    # fake joint groove depth
FAKE_EDGE  = 1.0    # how close the fake joint's groove comes to the outer curve

PART_GAP   = 20.0   # spacing between the parts in the viewport
# ------------------------------------------------------------------------------

MM = 0.1

TILE_FRONT = BRACKET_T + TILE_T          # tile fronts, measured from the frame back
TOP        = TILE_FRONT + RISE           # the flat
PROFILE_W  = 2.0 * FRAME_R + FLAT_W      # frame width seen from the front


def tree_outline(x0, cy, back=1.0):
    """Joint outline: trunk plus one equilateral triangle, trunk at x=x0 pointing +X,
    centered on y=cy."""
    tip_x = TRUNK_L + TRI_SIDE * math.sqrt(3.0) / 2.0
    top = [(-back, TRUNK_W / 2.0), (TRUNK_L, TRUNK_W / 2.0),  # trunk starts `back` inside the piece
           (TRUNK_L, TRI_SIDE / 2.0),                          # flat side of the triangle
           (tip_x, 0.0)]                                       # point
    bottom = [(px, -py) for px, py in reversed(top) if py > 0]
    return [(x0 + px, cy + py) for px, py in top + bottom]


def offset_outline(pts, d):
    """Grow a polygon outward by d, keeping sharp corners."""
    n = len(pts)
    area = sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1] for i in range(n))
    sgn = -1.0 if area < 0 else 1.0                      # outward side depends on winding

    def shifted(i):
        (x0, y0), (x1, y1) = pts[i], pts[(i + 1) % n]
        dx, dy = x1 - x0, y1 - y0
        ln = math.hypot(dx, dy)
        nx, ny = sgn * dy / ln, -sgn * dx / ln
        return (x0 + nx * d, y0 + ny * d), (dx, dy)

    lines = [shifted(i) for i in range(n)]
    out = []
    for i in range(n):
        (ax, ay), (adx, ady) = lines[i - 1]
        (bx, by), (bdx, bdy) = lines[i]
        den = adx * bdy - ady * bdx
        t = ((bx - ax) * bdy - (by - ay) * bdx) / den
        out.append((ax + adx * t, ay + ady * t))
    return out


def clip_outline(pts, keep):
    """The part of a polygon where keep(p) >= 0; keep is linear, so the cut is a line."""
    out = []
    for i in range(len(pts)):
        a, b = pts[i], pts[(i + 1) % len(pts)]
        ka, kb = keep(a), keep(b)
        if ka >= 0:
            out.append(a)
        if (ka >= 0) != (kb >= 0):
            t = ka / (ka - kb)
            out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    # a vertex right on the line comes out twice; a repeat would be a zero-length sketch line
    return [p for i, p in enumerate(out) if math.dist(p, out[i - 1]) > 1e-6]


def rotate(pts, deg):
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    return [(x * c - y * s, x * s + y * c) for x, y in pts]


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
        FO = adsk.fusion.FeatureOperations

        def plane_at(z):
            if abs(z) < 1e-9:
                return root.xYConstructionPlane
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * MM))
            return root.constructionPlanes.add(pin)

        def polygon_sketch(pts, z):
            sk = root.sketches.add(plane_at(z))
            ln = sk.sketchCurves.sketchLines
            P = lambda p: adsk.core.Point3D.create(p[0] * MM, p[1] * MM, 0)
            for i in range(len(pts)):
                ln.addByTwoPoints(P(pts[i]), P(pts[(i + 1) % len(pts)]))
            return sk

        def extrude(sk, z0, z1, operation, body=None):
            inp = exts.createInput(sk.profiles.item(0), operation)
            inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal((z1 - z0) * MM))
            if body is not None:
                inp.participantBodies = [body]
            return exts.add(inp)

        def near(cm, mm):
            return abs(cm - mm * MM) < 0.001

        def fillet(edges, r):
            fin = root.features.filletFeatures.createInput()
            fin.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(r * MM), False)
            root.features.filletFeatures.add(fin)

        def fillet_edges(body, pick):
            return fillet_list([body.edges.item(i) for i in range(body.edges.count)
                                if pick(body.edges.item(i))])

        def fillet_list(picked):
            # all at once so the corners blend; failing that, one feature per edge,
            # so one bad edge can't sink the rest. Returns the number skipped.
            coll = adsk.core.ObjectCollection.create()
            for e in picked:
                coll.add(e)
            try:
                fillet(coll, JOINT_R)
                return 0
            except:
                pass
            skipped = 0
            for tk in [e.entityToken for e in picked]:
                ents = design.findEntityByToken(tk)
                if not ents:
                    continue
                one = adsk.core.ObjectCollection.create()
                one.add(ents[0])
                try:
                    fillet(one, JOINT_R)
                except:
                    skipped += 1
            return skipped

        def new_body(pts, z0, z1):
            return extrude(polygon_sketch(pts, z0), z0, z1, FO.NewBodyFeatureOperation).bodies.item(0)

        def combine(target, tool, operation):
            tools = adsk.core.ObjectCollection.create()
            tools.add(tool)
            cin = root.features.combineFeatures.createInput(target, tools)
            cin.operation = operation
            return root.features.combineFeatures.add(cin)

        # --- straight piece: profile run along X ---
        oy = 0.0                                 # inner face
        piece_l = PITCH - CLEAR                  # the seam gap belongs to the next piece
        cy = oy + FRAME_R + FLAT_W / 2.0         # joint centered on the flat

        def profile_bar(x0, x1, inset, z0, name):
            # the profile run from x0 to x1; inset shrinks it (for the hollow back)
            xm = (x0 + x1) / 2.0
            pin = root.constructionPlanes.createInput()
            pin.setByOffset(root.yZConstructionPlane, adsk.core.ValueInput.createByReal(xm * MM))
            pl = root.constructionPlanes.add(pin)
            if pl.geometry.origin.x < 0:         # offset went -X; put the plane on the +X side
                pin = root.constructionPlanes.createInput()
                pin.setByOffset(root.yZConstructionPlane, adsk.core.ValueInput.createByReal(-xm * MM))
                pl = root.constructionPlanes.add(pin)
            sk = root.sketches.add(pl)
            px = pl.geometry.origin.x
            z1 = TOP - inset
            prof = [(oy + inset, z0), (oy + inset, z1), (oy + PROFILE_W - inset, z1), (oy + PROFILE_W - inset, z0)]
            SP = lambda p: sk.modelToSketchSpace(adsk.core.Point3D.create(px, p[0] * MM, p[1] * MM))
            for i in range(len(prof)):
                sk.sketchCurves.sketchLines.addByTwoPoints(SP(prof[i]), SP(prof[(i + 1) % len(prof)]))
            inp = exts.createInput(sk.profiles.item(0), FO.NewBodyFeatureOperation)
            inp.setSymmetricExtent(adsk.core.ValueInput.createByReal((x1 - x0) * MM), True)
            bar = exts.add(inp).bodies.item(0)
            bar.name = name

            # the big curve: fillet the two long top edges
            top_edges = adsk.core.ObjectCollection.create()
            for i in range(bar.edges.count):
                bb = bar.edges.item(i).boundingBox
                if near(bb.minPoint.z, z1) and near(bb.maxPoint.z, z1) and near(bb.maxPoint.x - bb.minPoint.x, x1 - x0):
                    top_edges.add(bar.edges.item(i))
            fillet(top_edges, FRAME_R - inset)
            return bar

        xc = PITCH / 2.0                         # the edge bracket, half a spacing in from the socket end
        piece = profile_bar(0.0, piece_l, 0.0, 0.0, 'StraightPiece')

        # tree on the far end, socket on the near end (the previous piece's tree ends at -CLEAR)
        tail = tree_outline(piece_l, cy)
        extrude(polygon_sketch(tail, FLOOR_T), FLOOR_T, TOP, FO.JoinFeatureOperation, piece)
        sock = offset_outline(tree_outline(-CLEAR, cy), CLEAR)
        extrude(polygon_sketch(sock, FLOOR_T), FLOOR_T, TOP + 1.0, FO.CutFeatureOperation, piece)
        sock_tip = max(p[0] for p in sock)

        # tab slot, open at the back, then the insert pocket in its ceiling
        slot_top = TAB_T + CLEAR
        hw = TAB_W / 2.0 + CLEAR
        slot = [(xc - hw, oy - 1.0), (xc + hw, oy - 1.0),
                (xc + hw, oy + SLOT_REACH + CLEAR), (xc - hw, oy + SLOT_REACH + CLEAR)]
        extrude(polygon_sketch(slot, -1.0), -1.0, slot_top, FO.CutFeatureOperation, piece)
        skc = root.sketches.add(plane_at(slot_top))
        skc.sketchCurves.sketchCircles.addByCenterRadius(
            adsk.core.Point3D.create(xc * MM, (oy + INSERT_Y) * MM, 0), (INSERT_HOLE_D / 2.0) * MM)
        extrude(skc, slot_top, slot_top + INSERT_DEPTH, FO.CutFeatureOperation, piece)

        # fillet the joint outlines on the flat, and the front edges of both end faces
        def piece_pick(e):
            bb = e.boundingBox
            outline = (near(bb.minPoint.z, TOP) and near(bb.maxPoint.z, TOP)
                       and (bb.minPoint.x >= (piece_l - 0.01) * MM or bb.maxPoint.x <= (sock_tip + 0.01) * MM))
            end_face = ((near(bb.minPoint.x, 0.0) and near(bb.maxPoint.x, 0.0))
                        or (near(bb.minPoint.x, piece_l) and near(bb.maxPoint.x, piece_l)))
            end_front = (end_face and bb.maxPoint.z > (TILE_FRONT + 0.01) * MM      # above the tile fronts
                         and bb.maxPoint.y - bb.minPoint.y > 0.001)                 # not a vertical edge
            return outline or end_front

        skipped = fillet_edges(piece, piece_pick)
        tree_end = max(p[0] for p in tail) - piece_l

        # hollow the back, leaving WALL all round; solid at the socket end, the tree end
        # and around the tab slot
        piece_vol = [piece.physicalProperties.volume]
        for h0, h1 in ((sock_tip + WALL, xc - hw - WALL), (xc + hw + WALL, piece_l - END_SOLID)):
            combine(piece, profile_bar(h0, h1, WALL, -1.0, 'Hollow'), FO.CutFeatureOperation)
        piece_vol.append(piece.physicalProperties.volume)

        # --- corner piece: built with the big corner's grid vertex at the origin, side A
        #     along +X and side B at +60 degrees, frame outside both; moved aside at the end ---
        S3 = math.sqrt(3.0)
        dB = (0.5, S3 / 2.0)                     # along side B
        nB = (-S3 / 2.0, 0.5)                    # out from side B
        a_end = PITCH / 2.0 - CLEAR              # arm A ends in a tree; the next piece starts at PITCH / 2
        b_end = PITCH / 2.0                      # arm B ends in a socket
        cy_c = -(INNER_OFF + FRAME_R + FLAT_W / 2.0)   # joint centerline, in side A's frame

        def build_v(inset, z0, name, ends=(a_end, b_end)):
            # each arm is its footprint extruded with the big curve on its long top edges;
            # the arms mirror each other across the miter, so they join with a crisp crease.
            # inset shrinks the whole shape (for the fake joint's groove depth and the hollow back);
            # ends are how far each arm runs from the corner
            z1, a_in, a_out = TOP - inset, INNER_OFF + inset, INNER_OFF + PROFILE_W - inset
            on_b = lambda s, a: (s * dB[0] + a * nB[0], s * dB[1] + a * nB[1])
            ci, co = (-S3 * a_in, -a_in), (-S3 * a_out, -a_out)   # where the arms meet, on the miter
            arms = [([ci, (ends[0], -a_in), (ends[0], -a_out), co], (1.0, 0.0)),
                    ([ci, on_b(ends[1], a_in), on_b(ends[1], a_out), co], dB)]
            bodies = []
            for outline, d in arms:
                b = new_body(outline, z0, z1)
                long_top = adsk.core.ObjectCollection.create()
                for i in range(b.edges.count):
                    e = b.edges.item(i)
                    p, q = e.startVertex.geometry, e.endVertex.geometry
                    if near(p.z, z1) and near(q.z, z1) and abs((q.x - p.x) * d[1] - (q.y - p.y) * d[0]) < 0.001:
                        long_top.add(e)
                fillet(long_top, FRAME_R - inset)
                bodies.append(b)
            combine(bodies[0], bodies[1], FO.JoinFeatureOperation)
            bodies[0].name = name
            return bodies[0]

        corner = build_v(0.0, 0.0, 'CornerPiece')
        extrude(polygon_sketch(tree_outline(a_end, cy_c), FLOOR_T), FLOOR_T, TOP, FO.JoinFeatureOperation, corner)
        # the last piece along side B travels toward the corner, so its tree root is CLEAR
        # past b_end; built in that piece's frame (side A's turned 240 degrees), then turned into place
        sock_c = offset_outline(tree_outline(-b_end - CLEAR, cy_c), CLEAR)
        extrude(polygon_sketch(rotate(sock_c, 240.0), FLOOR_T), FLOOR_T, TOP + 1.0,
                FO.CutFeatureOperation, corner)
        sock_c_tip = max(p[0] for p in sock_c)

        def corner_pick(e):
            # same edges as piece_pick, tested in each arm's own frame
            if e.startVertex is None:
                return False
            ends = [e.startVertex.geometry, e.endVertex.geometry]
            for deg, face_u, lo, hi in ((0.0, a_end, a_end - 0.01, a_end + 20.0),
                                        (240.0, -b_end, -b_end - 2.0, sock_c_tip + 0.01)):
                c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
                u = [(p.x * c + p.y * s) / MM for p in ends]
                v = [(-p.x * s + p.y * c) / MM for p in ends]
                z = [p.z / MM for p in ends]
                outline = (all(abs(zz - TOP) < 0.01 for zz in z) and all(lo <= uu <= hi for uu in u)
                           and all(abs(vv - cy_c) < FLAT_W / 2.0 for vv in v))
                end_front = (all(abs(uu - face_u) < 0.01 for uu in u) and max(z) > TILE_FRONT + 0.01
                             and abs(v[0] - v[1]) > 0.01)
                if outline or end_front:
                    return True
            return False

        skipped += fillet_edges(corner, corner_pick)

        # --- fake joint at the miter: the seam's groove, FAKE_D deep into every outer face.
        #     Like a real joint, the tree comes off square to the seam, into arm A. Built in
        #     a frame turned -60 degrees: u square to the miter, v back along it ---
        def fake_outlines(slide):
            t = tree_outline(0.0, -2.0 * slide, 2.0)          # trunk root on the miter, slid out along it
            return t, clip_outline(offset_outline(t, FAKE_W), lambda p: p[0])
        offs = [-y for x, y in rotate(fake_outlines(0.0)[1], -60.0)]                # out from side A
        slide = INNER_OFF + FRAME_R + FLAT_W - FAKE_EDGE - max(offs)               # out toward the tip
        fake_tree, ring = (rotate(o, -60.0) for o in fake_outlines(slide))
        m, w = (-S3 / 2.0, -0.5), (0.5, -S3 / 2.0)                 # along and across the miter
        t0, t1, h = -5.0, 2.0 * (INNER_OFF + PROFILE_W) + 5.0, FAKE_W / 2.0
        slab = [(m[0] * t + w[0] * k, m[1] * t + w[1] * k) for t, k in ((t0, -h), (t1, -h), (t1, h), (t0, h))]

        groove = new_body(ring, -1.0, TOP + 1.0)
        combine(groove, new_body(slab, -1.0, TOP + 1.0), FO.JoinFeatureOperation)
        combine(groove, new_body(fake_tree, -1.0, TOP + 1.0), FO.CutFeatureOperation)  # seam breaks at the trunk
        combine(groove, build_v(FAKE_D, -1.0, 'FakeCore'), FO.CutFeatureOperation)     # keep the outer skin only
        cut = combine(corner, groove, FO.CutFeatureOperation)

        # fillet the groove's lips: edges between the groove's faces and the original surface,
        # leaving out the back
        lips, fake_skipped = {}, 0
        try:
            made = set(f.tempId for f in cut.faces)
            for f in cut.faces:
                for e in f.edges:
                    p, q = e.startVertex, e.endVertex
                    on_back = p is not None and near(p.geometry.z, 0.0) and near(q.geometry.z, 0.0)
                    if not on_back and any(g.tempId not in made for g in e.faces):
                        lips[e.tempId] = e
            fake_skipped = fillet_list(list(lips.values()))
        except:
            fake_skipped = -1

        # hollow the back the same way, solid at both arm ends; the hollow runs through the miter
        corner_vol = [corner.physicalProperties.volume]
        combine(corner, build_v(WALL, -1.0, 'Hollow', (a_end - END_SOLID, -sock_c_tip - WALL)),
                FO.CutFeatureOperation)
        corner_vol.append(corner.physicalProperties.volume)

        # move it clear of the straight piece
        moved = adsk.core.ObjectCollection.create()
        moved.add(corner)
        mt = adsk.core.Matrix3D.create()
        mt.translation = adsk.core.Vector3D.create(
            (piece_l + tree_end + PART_GAP + S3 * (INNER_OFF + PROFILE_W)) * MM, 0, 0)
        mi = root.features.moveFeatures.createInput2(moved)
        mi.defineAsFreeMove(mt)
        root.features.moveFeatures.add(mi)

        ui.messageBox('Created StraightPiece and CornerPiece.\n'
                      'Triangle side %.0f, %.1f long with the trunk; socket floor %.0f; '
                      'clearance %.2f per side; joint fillet %.1f.\n'
                      'Piece %.2f long, %.0f wide, %.1f deep; slot %.1f wide, insert %.0fmm in.\n'
                      'CornerPiece arms %.2f (tree) and %.2f (socket) from the corner; '
                      'fake joint groove %.1f wide, %.1f deep.\n'
                      'Volume, solid -> hollowed: StraightPiece %.1f -> %.1f cm3, '
                      'CornerPiece %.1f -> %.1f cm3.\n'
                      'Joint fillet edges skipped: %d. Fake joint lip edges skipped: %s.'
                      % (TRI_SIDE, tree_end, FLOOR_T, CLEAR, JOINT_R,
                         piece_l, PROFILE_W, TOP, 2.0 * hw, INSERT_Y,
                         a_end, b_end, FAKE_W, FAKE_D,
                         piece_vol[0], piece_vol[1], corner_vol[0], corner_vol[1],
                         skipped, 'all (could not find them)' if fake_skipped < 0 else fake_skipped))
    except:
        if ui:
            ui.messageBox('Failed:\n{}'.format(traceback.format_exc()))
