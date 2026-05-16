import pcbnew

board = pcbnew.GetBoard()

footprints = {}
for fp in board.GetFootprints():
    footprints[fp.GetReference()] = fp

print("Placing capacitors under LEDs...")
for i in range(1, 25):
    led_ref = f"D{i}"
    cap_ref = f"C{i}"

    if led_ref not in footprints:
        print(f"Warning: {led_ref} not found!")
        continue
    if cap_ref not in footprints:
        print(f"Warning: {cap_ref} not found!")
        continue

    led = footprints[led_ref]
    cap = footprints[cap_ref]

    # Set same position as LED
    cap.SetPosition(led.GetPosition())

    # Flip to back if not already there
    if cap.GetLayer() == pcbnew.F_Cu:
        cap.Flip(cap.GetPosition(), False)

    # Match orientation
    cap.SetOrientation(led.GetOrientation())

pcbnew.Refresh()
print("Done!")