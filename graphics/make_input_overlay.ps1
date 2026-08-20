# Triangel input plate overlay - L-size sticker sheet for konbini photo print.
# Geometry is parsed from InputEnclosure.py so it cannot drift from the enclosure.
#
# Coordinate arrays are kept FLAT (x0,y0,x1,y1,...): PowerShell unrolls nested
# arrays on return, which silently collapses a one-pair list to a scalar.

Add-Type -AssemblyName System.Drawing

$ENCL = Join-Path $PSScriptRoot "..\hardware\input-enclosure\InputEnclosure.py"
$py   = Get-Content $ENCL -Raw

function PyNum([string]$name) {
    if ($py -match "(?m)^$name\s*=\s*(-?[0-9.]+)") { return [double]$Matches[1] }
    throw "could not parse $name from InputEnclosure.py"
}
function PyNum2([string]$n1, [string]$n2) {
    # e.g. "BOARD_W, BOARD_H = 80.0, 60.0"
    if ($py -match "(?m)^$n1\s*,\s*$n2\s*=\s*(-?[0-9.]+)\s*,\s*(-?[0-9.]+)") {
        return @([double]$Matches[1], [double]$Matches[2])
    }
    throw "could not parse $n1, $n2 from InputEnclosure.py"
}
function PyPairs([string]$name) {
    if ($py -notmatch "(?m)^$name\s*=\s*(\[[^\]]*\]|\([^)]*\))") { throw "could not parse $name" }
    $vals = New-Object System.Collections.Generic.List[double]
    foreach ($m in [regex]::Matches($Matches[1], '\(\s*(-?[0-9.]+)\s*,\s*(-?[0-9.]+)\s*\)')) {
        $vals.Add([double]$m.Groups[1].Value); $vals.Add([double]$m.Groups[2].Value)
    }
    if ($vals.Count -eq 0) { throw "no coordinate pairs in $name" }
    return $vals.ToArray()
}

# ---- geometry from the enclosure script ----
$board = PyNum2 'BOARD_W' 'BOARD_H'; $BOARD_W = $board[0]; $BOARD_H = $board[1]
$FIT = PyNum 'FIT'; $WALL = PyNum 'WALL'
$BTN_W = PyNum 'BTN_OPEN_W'; $BTN_H = PyNum 'BTN_OPEN_H'
$SW_W  = PyNum 'SW_SLOT_W';  $SW_H  = PyNum 'SW_SLOT_H'
$IR_W  = PyNum 'IR_OPEN_W';  $IR_H  = PyNum 'IR_OPEN_H'
$OPEN_R = PyNum 'OPEN_R'; $RIM_R = PyNum 'RIM_R'; $SCREW_HEAD = PyNum 'SCREW_HEAD'
$BUTTONS = PyPairs 'BUTTONS'      # up, down, left, right, centre
$SWITCH  = PyPairs 'SWITCH'
$SENSOR  = PyPairs 'SENSOR'
$HOLES   = PyPairs 'HOLES'        # TL, TR, BL, BR

$OFF     = $FIT + $WALL                 # board origin inside the plate
$PLATE_W = $BOARD_W + 2 * $OFF
$PLATE_H = $BOARD_H + 2 * $OFF

# plate-space centres
function BtnX([int]$i) { return $BUTTONS[2 * $i]     + $OFF }
function BtnY([int]$i) { return $BUTTONS[2 * $i + 1] + $OFF }
$SW_CX = $SWITCH[0] + $OFF; $SW_CY = $SWITCH[1] + $OFF
$IR_CX = $SENSOR[0] + $OFF; $IR_CY = $SENSOR[1] + $OFF

Write-Output "plate $PLATE_W x $PLATE_H mm, board offset $OFF"
Write-Output "openings: button $BTN_W x $BTN_H (R$OPEN_R), switch $SW_W x $SW_H, IR $IR_W x $IR_H, lead-in fillet $RIM_R"

# ---- overlay parameters ----
$KNIFE    = 0.7                          # slack for the blade, on top of the fillet
$CLEAR    = $RIM_R + $KNIFE              # total clearance per side around every opening
$INSET    = 3.0                          # sticker edge, in from the plate edge
$SCREW_CLR = 0.5                         # margin around each exposed screw countersink
$CORNER_R = 2.0                          # fillet on every corner of the sticker outline
$BLEED    = 0.7                          # black printed past the cut line, so the white
                                         # cut band has black on both sides
$CUT_LINE = 0.5                          # width of the white cut band
$LAT_A    = 19.0 / [Math]::Sqrt(3)       # lattice edge: 2 rows == the d-pad's 19mm arm
$TILE_GAP  = 0.45                        # tiles just clear of each other
$GRAPHIC   = "c:\code\triangel\graphics\triangel graphic.png"
$GFX_ALPHA  = 0.28

$SX0 = $INSET; $SY0 = $INSET; $SX1 = $PLATE_W - $INSET; $SY1 = $PLATE_H - $INSET
Write-Output ("sticker {0:N1} x {1:N1} mm, clearance {2:N1}mm per side, lattice edge {3:N3}mm" -f ($SX1-$SX0), ($SY1-$SY0), $CLEAR, $LAT_A)

# ---- sheet: L-size, overlay rotated 90 deg so the margins stay generous ----
$DPI = 600.0
$SHEET_W = 89.0; $SHEET_H = 127.0
function MM([double]$v) { return $v * $DPI / 25.4 }
$PX_W = [int][Math]::Round((MM $SHEET_W)); $PX_H = [int][Math]::Round((MM $SHEET_H))

$bmp = New-Object System.Drawing.Bitmap($PX_W, $PX_H, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$bmp.SetResolution($DPI, $DPI)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$g.Clear([System.Drawing.Color]::White)

$black  = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::Black)
$white  = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)
$penCut = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([float](MM $CUT_LINE))
$penCut.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
$family = New-Object System.Drawing.FontFamily "Arial"

# ---- path helpers, all in plate mm ----
function P([double]$x, [double]$y) { New-Object System.Drawing.PointF([float](MM $x), [float](MM $y)) }

# Fillet every corner of a closed polygon of right angles, convex and concave alike.
# For a right angle the arc centre is simply T1 + (T2 - V).
function RoundPolygonInto($src, $dst, [double]$r) {
    # PowerShell variable names are case-insensitive, so no $N alongside $n here
    $cnt = $src.Count
    for ($i = 0; $i -lt $cnt; $i++) {
        $prev = $src[($i - 1 + $cnt) % $cnt]; $cur = $src[$i]; $next = $src[($i + 1) % $cnt]
        $ix = $cur.X - $prev.X; $iy = $cur.Y - $prev.Y; $li = [Math]::Sqrt($ix * $ix + $iy * $iy)
        $ox = $next.X - $cur.X; $oy = $next.Y - $cur.Y; $lo = [Math]::Sqrt($ox * $ox + $oy * $oy)
        if ($li -eq 0 -or $lo -eq 0) { $dst.Add($cur); continue }
        $ix /= $li; $iy /= $li; $ox /= $lo; $oy /= $lo
        $rr = [Math]::Min($r, [Math]::Min($li, $lo) / 2)      # never eat a whole edge
        $t1x = $cur.X - $ix * $rr; $t1y = $cur.Y - $iy * $rr
        $t2x = $cur.X + $ox * $rr; $t2y = $cur.Y + $oy * $rr
        $cx = $t1x + ($t2x - $cur.X); $cy = $t1y + ($t2y - $cur.Y)
        $a0 = [Math]::Atan2($t1y - $cy, $t1x - $cx)
        $a1 = [Math]::Atan2($t2y - $cy, $t2x - $cx)
        $d = $a1 - $a0
        while ($d -gt [Math]::PI)  { $d -= 2 * [Math]::PI }
        while ($d -lt -[Math]::PI) { $d += 2 * [Math]::PI }
        for ($k = 0; $k -le 14; $k++) {
            $a = $a0 + $d * $k / 14
            $dst.Add((New-Object System.Drawing.PointF(
                [float]($cx + $rr * [Math]::Cos($a)), [float]($cy + $rr * [Math]::Sin($a)))))
        }
    }
}

# Square corner notches expose the four screws. A concave arc big enough to swallow
# the sticker's corner would eat half the short edge; the notch takes a smaller bite
# and is two straight cuts instead of a curve.
function OutlinePath([double]$grow) {
    $x0 = $SX0 - $grow; $y0 = $SY0 - $grow; $x1 = $SX1 + $grow; $y1 = $SY1 + $grow
    # notch reaches past the far side of the screw head, less the bleed
    $nx = $HOLES[0] + $OFF + $SCREW_HEAD / 2 + $SCREW_CLR - $grow
    $ny = $HOLES[1] + $OFF + $SCREW_HEAD / 2 + $SCREW_CLR - $grow
    $nxr = $PLATE_W - $nx; $nyb = $PLATE_H - $ny

    $L = New-Object 'System.Collections.Generic.List[System.Drawing.PointF]'
    $L.Add((P $nx  $y0)); $L.Add((P $nxr $y0))          # top edge
    $L.Add((P $nxr $ny)); $L.Add((P $x1  $ny))          # top-right notch
    $L.Add((P $x1  $nyb)); $L.Add((P $nxr $nyb))        # right edge, bottom-right notch
    $L.Add((P $nxr $y1)); $L.Add((P $nx  $y1))          # bottom edge
    $L.Add((P $nx  $nyb)); $L.Add((P $x0  $nyb))        # bottom-left notch
    $L.Add((P $x0  $ny)); $L.Add((P $nx  $ny))          # left edge, top-left notch

    $R = New-Object 'System.Collections.Generic.List[System.Drawing.PointF]'
    RoundPolygonInto $L $R (MM $CORNER_R)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddPolygon($R.ToArray())
    return $path
}

function RoundRectPath([double]$cx, [double]$cy, [double]$w, [double]$h, [double]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $x = $cx - $w / 2; $y = $cy - $h / 2
    $d = [float](MM (2 * $r))
    $p.AddArc([float](MM $x), [float](MM $y), $d, $d, 180, 90)
    $p.AddArc([float](MM ($x + $w - 2 * $r)), [float](MM $y), $d, $d, 270, 90)
    $p.AddArc([float](MM ($x + $w - 2 * $r)), [float](MM ($y + $h - 2 * $r)), $d, $d, 0, 90)
    $p.AddArc([float](MM $x), [float](MM ($y + $h - 2 * $r)), $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

# ---- openings, in plate coords ----
$btnCutW = $BTN_W + 2 * $CLEAR; $btnCutH = $BTN_H + 2 * $CLEAR; $btnCutR = $OPEN_R + $CLEAR
$irCutW  = $IR_W  + 2 * $CLEAR; $irCutH  = $IR_H  + 2 * $CLEAR
$swCutW  = $SW_W  + 2 * $CLEAR; $swCutH  = $SW_H  + 2 * $CLEAR

Write-Output "cut openings:"
for ($i = 0; $i -lt 5; $i++) {
    Write-Output ("   button  {0,5:N2} x {1,-5:N2} R{2,-4:N2} at ({3:N2}, {4:N2})" -f $btnCutW, $btnCutH, $btnCutR, (BtnX $i), (BtnY $i))
}
Write-Output ("   IR      {0,5:N2} x {1,-5:N2} R{2,-4:N2} at ({3:N2}, {4:N2})" -f $irCutW, $irCutH, $btnCutR, $IR_CX, $IR_CY)
Write-Output ("   switch  {0,5:N2} x {1,-5:N2} R{2,-4:N2} at ({3:N2}, {4:N2})  obround" -f $swCutW, $swCutH, ($swCutH / 2), $SW_CX, $SW_CY)

# narrowest web: the switch slot's left end vs the right button's top-right corner arc
$bArcX = (BtnX 3) + $btnCutW / 2 - $btnCutR
$bArcY = (BtnY 3) - $btnCutH / 2 + $btnCutR
$sArcX = $SW_CX - ($swCutW - $swCutH) / 2
$web = [Math]::Sqrt([Math]::Pow($bArcX - $sArcX, 2) + [Math]::Pow($bArcY - $SW_CY, 2)) - $btnCutR - $swCutH / 2
Write-Output ("narrowest web (switch slot to right button): {0:N2} mm" -f $web)

# ---- draw, rotated 90 deg onto the sheet ----
$state = $g.Save()
$g.TranslateTransform([float](MM ($SHEET_W / 2)), [float](MM ($SHEET_H / 2)))
$g.RotateTransform(90)
$g.TranslateTransform([float](-(MM ($PLATE_W / 2))), [float](-(MM ($PLATE_H / 2))))

$cutPath = OutlinePath 0.0
$g.FillPath($black, $cutPath)
$g.SetClip($cutPath, [System.Drawing.Drawing2D.CombineMode]::Intersect)

# the triangel graphic, knocked well back. Its white triangle drops to a dark grey and
# the internal linework stays black, so it reads as a watermark rather than a shape.
$gfx = [System.Drawing.Image]::FromFile($GRAPHIC)

# The triangle does not fill its square canvas, so measure it rather than assuming:
# downsample, then take the bounding box of everything that is not background black.
$probeN = 128
$probe  = New-Object System.Drawing.Bitmap($probeN, $probeN)
$pg = [System.Drawing.Graphics]::FromImage($probe)
$pg.DrawImage($gfx, 0, 0, $probeN, $probeN); $pg.Dispose()
$mnx = $probeN; $mny = $probeN; $mxx = -1; $mxy = -1
for ($py = 0; $py -lt $probeN; $py++) {
    for ($px = 0; $px -lt $probeN; $px++) {
        $c = $probe.GetPixel($px, $py)
        if (($c.R + $c.G + $c.B) -gt 120) {
            if ($px -lt $mnx) { $mnx = $px }; if ($px -gt $mxx) { $mxx = $px }
            if ($py -lt $mny) { $mny = $py }; if ($py -gt $mxy) { $mxy = $py }
        }
    }
}
$probe.Dispose()
$fx0 = $mnx / $probeN; $fx1 = ($mxx + 1) / $probeN
$fy0 = $mny / $probeN; $fy1 = ($mxy + 1) / $probeN
Write-Output ("graphic occupies x {0:P0}..{1:P0}, y {2:P0}..{3:P0} of its canvas" -f $fx0, $fx1, $fy0, $fy1)

# Pre-scale once. Tiling straight from the 5906px original would resample 35 megapixels
# per tile, roughly a hundred times over.
$tileN = 512
$tileImg = New-Object System.Drawing.Bitmap($tileN, $tileN, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$tgc = [System.Drawing.Graphics]::FromImage($tileImg)
$tgc.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$tgc.DrawImage($gfx, 0, 0, $tileN, $tileN)
$tgc.Dispose(); $gfx.Dispose()

$cm = New-Object System.Drawing.Imaging.ColorMatrix
$cm.Matrix33 = $GFX_ALPHA
$ia = New-Object System.Drawing.Imaging.ImageAttributes
$ia.SetColorMatrix($cm)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic

# canvas size that makes the triangle inside it exactly one cell wide, less the gap
# shrinking a triangle of side a to a' about its centroid opens a perpendicular
# gap of (a - a')/sqrt(3) on every edge, so invert that to hit TILE_GAP exactly
$triW = $LAT_A - $TILE_GAP * [Math]::Sqrt(3)
$S    = $triW / ($fx1 - $fx0)

$triH = $triW * [Math]::Sqrt(3) / 2.0
# The artwork's own triangle centroid, as a fraction of its canvas. A triangle's centroid
# is a third of the way up from the base, not halfway - placing by bounding-box centre
# instead leaves the row gaps noticeably wider than the two diagonal families.
$gcx = ($fx0 + $fx1) / 2
$gcy = $fy0 + 2.0 * ($fy1 - $fy0) / 3.0

function DrawTile([double]$ccx, [double]$ccy, [bool]$down) {
    $st = $g.Save()
    # Clip to this cell's triangle. The artwork's canvas is square with black corners
    # outside the triangle; unclipped they paint over the neighbouring tiles and darken
    # them into bands.
    $v = New-Object 'System.Drawing.PointF[]' 3
    if ($down) {
        $v[0] = P ($ccx - $triW / 2) ($ccy - $triH / 3)
        $v[1] = P ($ccx + $triW / 2) ($ccy - $triH / 3)
        $v[2] = P $ccx               ($ccy + 2 * $triH / 3)
    } else {
        $v[0] = P $ccx               ($ccy - 2 * $triH / 3)
        $v[1] = P ($ccx + $triW / 2) ($ccy + $triH / 3)
        $v[2] = P ($ccx - $triW / 2) ($ccy + $triH / 3)
    }
    $clip = New-Object System.Drawing.Drawing2D.GraphicsPath
    $clip.AddPolygon($v)
    $g.SetClip($clip, [System.Drawing.Drawing2D.CombineMode]::Intersect)
    $clip.Dispose()

    if ($down) {                                  # the artwork points up; flip for down cells
        $g.TranslateTransform([float](MM $ccx), [float](MM $ccy))
        $g.RotateTransform(180)
        $g.TranslateTransform([float](-(MM $ccx)), [float](-(MM $ccy)))
    }
    $dx = $ccx - $gcx * $S
    $dy = $ccy - $gcy * $S
    $r = New-Object System.Drawing.Rectangle(
        [int][Math]::Round((MM $dx)), [int][Math]::Round((MM $dy)),
        [int][Math]::Round((MM $S)),  [int][Math]::Round((MM $S)))
    $g.DrawImage($tileImg, $r, 0, 0, $tileImg.Width, $tileImg.Height,
                 [System.Drawing.GraphicsUnit]::Pixel, $ia)
    $g.Restore($st)
}

# Same lattice the tiles sat on before - a vertex on the centre button, two rows to the
# d-pad's 19mm arm - but now every cell carries the graphic, alternating up and down.
$ox = BtnX 4; $oy = BtnY 4
$h3 = $LAT_A * [Math]::Sqrt(3) / 2.0
$nTiles = 0
for ($j = -8; $j -le 8; $j++) {
    $cy = $oy + ($j + 0.5) * $h3
    if ($cy -lt -15 -or $cy -gt $PLATE_H + 15) { continue }
    for ($i = -8; $i -le 8; $i++) {
        # DrawTile takes centroids: a third of the way from the flat edge, not the middle
        $cxd = $ox + ($i + 0.5) * $LAT_A + $j * $LAT_A / 2.0
        if ($cxd -gt -15 -and $cxd -lt $PLATE_W + 15) { DrawTile $cxd ($cy - $h3 / 6.0) $true;  $nTiles++ }
        $cxu = $cxd + $LAT_A / 2.0
        if ($cxu -gt -15 -and $cxu -lt $PLATE_W + 15) { DrawTile $cxu ($cy + $h3 / 6.0) $false; $nTiles++ }
    }
}
$g.ResetClip()
$tileImg.Dispose(); $ia.Dispose()
Write-Output ("tiled $nTiles triangles at {0:N2}mm edge ({1:N2}mm gap) at alpha {2:N2}" -f $LAT_A, $TILE_GAP, $GFX_ALPHA)

# cut bands on the openings only - the outline is cut along the black edge itself
for ($i = 0; $i -lt 5; $i++) {
    $p = RoundRectPath (BtnX $i) (BtnY $i) $btnCutW $btnCutH $btnCutR
    $g.DrawPath($penCut, $p); $p.Dispose()
}
$p = RoundRectPath $IR_CX $IR_CY $irCutW $irCutH $btnCutR; $g.DrawPath($penCut, $p); $p.Dispose()
$p = RoundRectPath $SW_CX $SW_CY $swCutW $swCutH ($swCutH / 2); $g.DrawPath($penCut, $p); $p.Dispose()

# ---- labels ----
function Label([string]$txt, [double]$cx, [double]$cy, [double]$capMM, [double]$maxMM) {
    $p  = New-Object System.Drawing.Drawing2D.GraphicsPath
    $sf = [System.Drawing.StringFormat]::GenericTypographic
    $p.AddString($txt, $family, [int][System.Drawing.FontStyle]::Bold, 200.0,
                 (New-Object System.Drawing.PointF(0, 0)), $sf)
    $b = $p.GetBounds()
    $s = (MM $capMM) / $b.Height
    if ($b.Width * $s -gt (MM $maxMM)) { $s = (MM $maxMM) / $b.Width }
    $m = New-Object System.Drawing.Drawing2D.Matrix
    $m.Scale([float]$s, [float]$s); $p.Transform($m); $m.Dispose()
    $b = $p.GetBounds()
    $m2 = New-Object System.Drawing.Drawing2D.Matrix
    $m2.Translate([float]((MM $cx) - ($b.X + $b.Width / 2)), [float]((MM $cy) - ($b.Y + $b.Height / 2)))
    $p.Transform($m2); $m2.Dispose()
    $g.FillPath($white, $p); $p.Dispose()
}

Label "BRIGHTER" 34.5  6.0 1.8 22
Label "DIMMER"   34.5 44.0 1.8 22
# PREV / HOLD / NEXT all sit on one line above the d-pad's middle row
Label "PREV"     15.5 25.0 1.8 11
Label "HOLD"     34.5 25.0 1.8 11
Label "NEXT"     53.5 25.0 1.8 11
function InkWidthMM([string]$txt, [double]$capMM) {
    $p  = New-Object System.Drawing.Drawing2D.GraphicsPath
    $sf = [System.Drawing.StringFormat]::GenericTypographic
    $p.AddString($txt, $family, [int][System.Drawing.FontStyle]::Bold, 200.0,
                 (New-Object System.Drawing.PointF(0, 0)), $sf)
    $b = $p.GetBounds(); $p.Dispose()
    return $capMM * $b.Width / $b.Height
}

# One block above the slot, the whole thing centred on the switch. ON/AUTO/OFF clears
# the slot's cut band by 1.6mm, the same gap the button labels keep from their outlines.
# ON and OFF are different widths, so place them by their ink edges rather than their
# centres - centring the two differently-sized words would push the row off the switch.
$swRowY = 16.45; $swCap = 1.4; $swSpan = 15.5
$rowL = $SW_CX - $swSpan / 2
$rowR = $SW_CX + $swSpan / 2
$wOn = InkWidthMM "ON" $swCap; $wAuto = InkWidthMM "AUTO" $swCap; $wOff = InkWidthMM "OFF" $swCap
$swGap = ($swSpan - ($wOn + $wAuto + $wOff)) / 2
$onX   = $rowL + $wOn / 2
$autoX = $rowL + $wOn + $swGap + $wAuto / 2
$offX  = $rowR - $wOff / 2
Write-Output ("switch block spans {0:N2}..{1:N2}mm, centred on the slot at {2:N2}mm" -f $rowL, $rowR, $SW_CX)
Write-Output ("   ON {0:N2}mm  AUTO {1:N2}mm  OFF {2:N2}mm, equal gaps of {3:N2}mm" -f $wOn, $wAuto, $wOff, $swGap)

Label "SOUND"    $SW_CX 10.25 1.6 12
Label "REACTIVE" $SW_CX 13.35 1.6 12
Label "ON"       $onX   $swRowY $swCap 4
Label "AUTO"     $autoX $swRowY $swCap 6
Label "OFF"      $offX  $swRowY $swCap 5

$g.Restore($state)


# ---- save ----
$out = $PSScriptRoot
$png = Join-Path $out "triangel_input_overlay_L.png"
$jpg = Join-Path $out "triangel_input_overlay_L.jpg"
$bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
$enc = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$ep = New-Object System.Drawing.Imaging.EncoderParameters 1
$ep.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), ([long]95)
$bmp.Save($jpg, $enc, $ep)
$g.Dispose(); $bmp.Dispose(); $penCut.Dispose()
Write-Output ""
Write-Output "sheet $SHEET_W x $SHEET_H mm at $DPI dpi = $PX_W x $PX_H px"
Write-Output "wrote $png"
Write-Output "wrote $jpg"
