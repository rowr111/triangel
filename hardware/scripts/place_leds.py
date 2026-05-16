import pcbnew, math

board = pcbnew.GetBoard()

# Inner triangle vertices (mm)
v1 = (108.660, 145.0)  # bottom-left
v2 = (191.340, 145.0)  # bottom-right
v3 = (150.0,   73.397) # top

def lerp(p1, p2, t):
    return (p1[0] + (p2[0]-p1[0])*t, p1[1] + (p2[1]-p1[1])*t)

# Generate 24 positions with rotation (9 per side, corners shared)
positions = []
for i in range(9):   positions.append((*lerp(v1, v2, i/8), 0))    # bottom edge
for i in range(1,9): positions.append((*lerp(v2, v3, i/8), -60))  # right edge
for i in range(1,8): positions.append((*lerp(v3, v1, i/8), 60))   # left edge

# Corner angles
corner_angles = {0: 45, 8: -45, 16: 0}
for idx, angle in corner_angles.items():
    x, y, _ = positions[idx]
    positions[idx] = (x, y, angle)

# Build dict of footprints by reference
footprints = {}
for fp in board.GetFootprints():
    footprints[fp.GetReference()] = fp

print(f"Placing {len(positions)} LEDs...")
for i, (x, y, angle) in enumerate(positions):
    ref = f"D{i+1}"
    if ref not in footprints:
        print(f"Warning: {ref} not found!")
        continue
    fp = footprints[ref]
    fp.SetPosition(pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y)))
    fp.SetOrientation(pcbnew.EDA_ANGLE(angle, pcbnew.DEGREES_T))

pcbnew.Refresh()
print("Done!")