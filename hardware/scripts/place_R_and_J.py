import pcbnew

board = pcbnew.GetBoard()

d1  = (108.660, 145.0)
d24 = (113.828, 136.050)

mx = (d1[0] + d24[0]) / 2
my = (d1[1] + d24[1]) / 2

dx = d24[0] - d1[0]
dy = d24[1] - d1[1]
length = (dx**2 + dy**2) ** 0.5
px, py = -dy / length, dx / length

offset = 1.25

components = {
    "R1": (mx + px * offset, my + py * offset, 60),
    "R2": (mx - px * offset, my - py * offset, 60),
    "J1": (142.0, 118.0, 90),
    "J2": (158.0, 118.0, 90),
}

footprints = {}
for fp in board.GetFootprints():
    footprints[fp.GetReference()] = fp

for ref, (x, y, angle) in components.items():
    if ref not in footprints:
        print(f"Warning: {ref} not found!")
        continue
    fp = footprints[ref]
    fp.SetPosition(pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y)))
    if fp.GetLayer() == pcbnew.F_Cu:
        fp.Flip(fp.GetPosition(), False)
    fp.SetOrientation(pcbnew.EDA_ANGLE(angle, pcbnew.DEGREES_T))

pcbnew.Refresh()
print("Done!")