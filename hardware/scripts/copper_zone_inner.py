import pcbnew

board = pcbnew.GetBoard()

vertices = [
    (117.320, 140.0),
    (182.680, 140.0),
    (150.0,   83.397),
]

zone = pcbnew.ZONE(board)
zone.SetLayer(pcbnew.F_Cu)
zone.SetNetCode(0)

# Set clearance very small so it fills into corners
zone.SetLocalClearance(pcbnew.FromMM(0.0))
zone.SetMinThickness(pcbnew.FromMM(0.1))

outline = zone.Outline()
outline.NewOutline()
for x, y in vertices:
    outline.Append(pcbnew.FromMM(x), pcbnew.FromMM(y))

board.Add(zone)

filler = pcbnew.ZONE_FILLER(board)
filler.Fill(board.Zones())

pcbnew.Refresh()
print("Done!")