import pcbnew, math

board = pcbnew.GetBoard()

# Triangle settings
side_mm = 100  # 10cm sides
cx, cy = 0, 0  # center position in mm - adjust if needed

# Calculate vertices
h = side_mm * math.sqrt(3) / 2
v1 = (cx,            cy - h * 2/3)  # top
v2 = (cx - side_mm/2, cy + h * 1/3)  # bottom left
v3 = (cx + side_mm/2, cy + h * 1/3)  # bottom right

def add_line(x1, y1, x2, y2):
    seg = pcbnew.PCB_SHAPE(board)
    seg.SetShape(pcbnew.SHAPE_T_SEGMENT)
    seg.SetLayer(pcbnew.Edge_Cuts)
    seg.SetStart(pcbnew.VECTOR2I(pcbnew.FromMM(x1), pcbnew.FromMM(y1)))
    seg.SetEnd(pcbnew.VECTOR2I(pcbnew.FromMM(x2), pcbnew.FromMM(y2)))
    seg.SetWidth(pcbnew.FromMM(0.05))
    board.Add(seg)

add_line(v1[0], v1[1], v2[0], v2[1])
add_line(v2[0], v2[1], v3[0], v3[1])
add_line(v3[0], v3[1], v1[0], v1[1])

pcbnew.Refresh()