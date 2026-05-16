import pcbnew

board = pcbnew.GetBoard()

v1 = (117.320, 140.0)
v2 = (182.680, 140.0)
v3 = (150.0,   83.397)

def add_line(x1, y1, x2, y2):
    seg = pcbnew.PCB_SHAPE(board)
    seg.SetShape(pcbnew.SHAPE_T_SEGMENT)
    seg.SetLayer(pcbnew.Eco1_User)
    seg.SetStart(pcbnew.VECTOR2I(pcbnew.FromMM(x1), pcbnew.FromMM(y1)))
    seg.SetEnd(pcbnew.VECTOR2I(pcbnew.FromMM(x2), pcbnew.FromMM(y2)))
    seg.SetWidth(pcbnew.FromMM(0.05))
    board.Add(seg)

add_line(v1[0], v1[1], v2[0], v2[1])
add_line(v2[0], v2[1], v3[0], v3[1])
add_line(v3[0], v3[1], v1[0], v1[1])

pcbnew.Refresh()
print("Done!")