# graphics

Artwork and the assembly sticker sheets. Every sheet is 89 x 127 mm - Japanese L size - at
600 dpi.

| File | What |
|---|---|
| `triangel graphic.*` | artwork for triangle shapes |
| `triangel_stickers_L.*` | Tile orientation stickers, 12 mm, 25 numbered + 5 blanks |
| `triangel_input_overlay_L.*` | Input plate overlay, 81 x 61 mm, 1 per sheet |
| `triangel_remote_overlay_L.*` | IR remote overlay, 31 x 51 mm, 4 per sheet |

## Printing

Convenience store photo print on L sticker paper, not document print. PNG and JPG are the
same image - use the JPG if the machine will not take PNG.

## Regenerating

    pwsh graphics\make_input_overlay.ps1
    pwsh graphics\make_remote_overlay.ps1

Windows PowerShell only - they draw with `System.Drawing` - and they write the PNG and JPG
next to themselves. `make_input_overlay.ps1` parses
`../hardware/input-enclosure/InputEnclosure.py`, so the overlay tracks the enclosure.
