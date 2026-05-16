import pcbnew, math

board = pcbnew.GetBoard()

centroid = (150.0, 121.132)
R = 31.132  # circumradius mm

holes = {
    "H1": (150.0, 90.0),
    "H2": (centroid[0] + R * math.sqrt(3)/2, centroid[1] + R / 2),
    "H3": (centroid[0] - R * math.sqrt(3)/2, centroid[1] + R / 2),
}

footprints = {}
for fp in board.GetFootprints():
    footprints[fp.GetReference()] = fp

for ref, (x, y) in holes.items():
    if ref not in footprints:
        print(f"Warning: {ref} not found!")
        continue
    footprints[ref].SetPosition(pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y)))
    print(f"{ref} placed at ({x:.3f}, {y:.3f})")

pcbnew.Refresh()
print("Done!")