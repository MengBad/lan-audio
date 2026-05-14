# Generate demo screenshots for README using .NET System.Drawing
Add-Type -AssemblyName System.Drawing

$screenshotsDir = Join-Path $PSScriptRoot "..\screenshots"
New-Item -ItemType Directory -Force -Path $screenshotsDir | Out-Null

# Color palette - modern dark theme
$bg       = [System.Drawing.Color]::FromArgb(18, 18, 24)
$cardBg   = [System.Drawing.Color]::FromArgb(28, 28, 38)
$accent   = [System.Drawing.Color]::FromArgb(99, 102, 241)
$grn      = [System.Drawing.Color]::FromArgb(34, 197, 94)
$ylw      = [System.Drawing.Color]::FromArgb(234, 179, 8)
$red      = [System.Drawing.Color]::FromArgb(239, 68, 68)
$txtPri   = [System.Drawing.Color]::FromArgb(248, 250, 252)
$txtSec   = [System.Drawing.Color]::FromArgb(148, 163, 184)
$txtMut   = [System.Drawing.Color]::FromArgb(100, 116, 139)
$wht      = [System.Drawing.Color]::White
$blk      = [System.Drawing.Color]::Black
$darkBg   = [System.Drawing.Color]::FromArgb(12, 12, 16)
$chipBg   = [System.Drawing.Color]::FromArgb(40, 40, 55)
$purp     = [System.Drawing.Color]::FromArgb(168, 85, 247)
$teal     = [System.Drawing.Color]::FromArgb(16, 185, 129)

function New-SB {
    param([int]$W = 400, [int]$H = 720)
    $b = New-Object System.Drawing.Bitmap($W, $H)
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAlias
    return @{ Bmp = $b; G = $g; W = $W; H = $H }
}

function RRect {
    param($G, $Brush, $X, $Y, $W, $H, $R = 12)
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $R * 2
    $p.AddArc($X, $Y, $d, $d, 180, 90)
    $p.AddArc(($X + $W - $d), $Y, $d, $d, 270, 90)
    $p.AddArc(($X + $W - $d), ($Y + $H - $d), $d, $d, 0, 90)
    $p.AddArc($X, ($Y + $H - $d), $d, $d, 90, 90)
    $p.CloseFigure()
    $G.FillPath($Brush, $p)
    $p.Dispose()
}

function CTxt {
    param($G, $Text, $Font, $Brush, $X, $Y, $W, $H)
    $rf = New-Object System.Drawing.RectangleF($X, $Y, $W, $H)
    $sf = New-Object System.Drawing.StringFormat
    $sf.Alignment = [System.Drawing.StringAlignment]::Center
    $sf.LineAlignment = [System.Drawing.StringAlignment]::Center
    $G.DrawString($Text, $Font, $Brush, $rf, $sf)
    $sf.Dispose()
}

function Txt {
    param($G, $Text, $Font, $Brush, $X, $Y)
    $G.DrawString($Text, $Font, $Brush, [System.Drawing.PointF]::new($X, $Y))
}

function MakePen {
    param($Color, $Width = 1, $Dash = $false)
    $pen = New-Object System.Drawing.Pen($Color, $Width)
    if ($Dash) { $pen.DashStyle = [System.Drawing.Drawing2D.DashStyle]::Dash }
    return $pen
}

function Solid { param($C) return New-Object System.Drawing.SolidBrush($C) }
function FontB { param($S, $Sz = 10) return New-Object System.Drawing.Font("Segoe UI", $Sz, [System.Drawing.FontStyle]::Bold) }
function FontN { param($Sz = 10) return New-Object System.Drawing.Font("Segoe UI", $Sz) }
function FontM { param($Sz = 9) return New-Object System.Drawing.Font("Consolas", $Sz) }

# ============================================================
# 1. Desktop Sender (600x450)
# ============================================================
Write-Host "Generating desktop sender screenshot..."
$r = New-SB -W 600 -H 450
$g = $r.G
$g.FillRectangle((Solid $bg), 0, 0, 600, 450)

Txt $g "LAN Audio - Sender" (FontB 14) (Solid $txtPri) 20 16
$g.FillEllipse((Solid $grn), 200, 22, 10, 10)
Txt $g "Streaming" (FontN 10) (Solid $grn) 216 18

# Audio Source card
RRect $g (Solid $cardBg) 16 50 270 140
Txt $g "Audio Source" (FontB 11) (Solid $txtPri) 28 60
Txt $g "Source:   Windows Loopback (WASAPI)" (FontM 9) (Solid $grn) 28 88
Txt $g "Codec:    Opus @ 48kHz, 32ms frames" (FontM 9) (Solid $txtSec) 28 108
Txt $g "Bitrate:  128 kbps (VBR)" (FontM 9) (Solid $txtSec) 28 128
Txt $g "Protocol: v2_header" (FontM 9) (Solid $txtSec) 28 148

# Connected Clients card
RRect $g (Solid $cardBg) 300 50 284 140
Txt $g "Connected Clients" (FontB 11) (Solid $txtPri) 312 60
Txt $g "Xiaomi 24129PN74C" (FontN 10) (Solid $grn) 312 88
Txt $g "  Latency: 64ms (p95) | Mode: low_latency" (FontN 9) (Solid $txtSec) 312 110
Txt $g "  Transport: WiFi | Buffer: 2 frames" (FontN 9) (Solid $txtSec) 312 128
Txt $g "Pixel 8 Pro" (FontN 10) (Solid $grn) 312 152
Txt $g "  Latency: 185ms (p95) | Mode: balanced" (FontN 9) (Solid $txtSec) 312 174

# Jitter Graph card
RRect $g (Solid $cardBg) 16 205 568 120
Txt $g "Jitter (ms) - Last 120 samples" (FontB 11) (Solid $txtPri) 28 215
$pg = MakePen $grn 2; $py = MakePen $ylw 2; $pr = MakePen $red 2
$pts = @()
for ($i = 0; $i -lt 100; $i++) {
    $x = 40.0 + $i * 5.2
    $jv = [Math]::Abs((Get-Random -Minimum -10 -Maximum 40)) * 1.5
    $y = 300.0 - $jv
    $pts += @{ X = $x; Y = $y; V = $jv }
}
for ($i = 1; $i -lt $pts.Count; $i++) {
    $pen = if ($pts[$i].V -lt 30) { $pg } elseif ($pts[$i].V -lt 60) { $py } else { $pr }
    $g.DrawLine($pen, [float]$pts[$i-1].X, [float]$pts[$i-1].Y, [float]$pts[$i].X, [float]$pts[$i].Y)
}
$dp = MakePen $txtMut 1 $true
$g.DrawLine($dp, 40, 280, 555, 280)
Txt $g "p50=48ms  p95=64ms" (FontN 9) (Solid $txtMut) 430 282

# Controls card
RRect $g (Solid $cardBg) 16 340 568 95
Txt $g "Playback Controls" (FontB 11) (Solid $txtPri) 28 350
$btns = @(
    @{L="Low Latency"; C=$grn; X=28},
    @{L="Balanced"; C=$ylw; X=188},
    @{L="High Quality"; C=$accent; X=348}
)
foreach ($b in $btns) {
    RRect $g (Solid $b.C) $b.X 375 140 36 8
    CTxt $g $b.L (FontN 10) (Solid $wht) $b.X 375 140 36
}
RRect $g (Solid $red) 520 375 52 36 8
CTxt $g "Stop" (FontN 10) (Solid $wht) 520 375 52 36

$r.Bmp.Save((Join-Path $screenshotsDir "screenshot-desktop-sender.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$r.G.Dispose(); $r.Bmp.Dispose()
Write-Host "  -> desktop sender done"

# ============================================================
# 2. Android Discovery (400x700)
# ============================================================
Write-Host "Generating Android discovery screenshot..."
$r = New-SB -W 400 -H 700
$g = $r.G
$g.FillRectangle((Solid $bg), 0, 0, 400, 700)
$g.FillRectangle((Solid $darkBg), 0, 0, 400, 28)
Txt $g "12:30" (FontN 9) (Solid $txtSec) 16 6
Txt $g "WiFi   BT   [|||]" (FontN 9) (Solid $txtSec) 280 6

Txt $g "LAN Audio" (FontB 18) (Solid $txtPri) 20 45

# Hero orb - disconnected
$ox = 140; $oy = 95; $os = 120
$og = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.PointF($ox, $oy)),
    (New-Object System.Drawing.PointF(($ox + $os), ($oy + $os))),
    $accent, $purp)
$g.FillEllipse($og, $ox, $oy, $os, $os); $og.Dispose()
$g.DrawEllipse((MakePen ([System.Drawing.Color]::FromArgb(60, 99, 102, 241)) 3), ($ox - 8), ($oy - 8), ($os + 16), ($os + 16))
CTxt $g "Disconnected" (FontB 12) (Solid $txtPri) $ox ($oy + 45) $os 30

# Nearby Senders card
RRect $g (Solid $cardBg) 16 240 368 180
Txt $g "Nearby Senders (mDNS)" (FontB 11) (Solid $txtPri) 28 252

$srvrs = @(
    @{N="DESKTOP-PC"; A="192.168.1.100:7878"; Q="Excellent"; L="2ms"},
    @{N="LivingRoom-PC"; A="192.168.1.105:7878"; Q="Good"; L="8ms"}
)
$sy = 280
foreach ($sv in $srvrs) {
    RRect $g (Solid $chipBg) 28 $sy 340 50 8
    Txt $g $sv.N (FontB 11) (Solid $txtPri) 42 ($sy + 6)
    Txt $g $sv.A (FontN 9) (Solid $txtSec) 42 ($sy + 28)
    $g.FillEllipse((Solid $grn), 340, ($sy + 12), 8, 8)
    Txt $g $sv.L (FontN 9) (Solid $grn) 352 ($sy + 8)
    $sy += 58
}

# Manual Connect card
RRect $g (Solid $cardBg) 16 435 368 70
Txt $g "Manual Connect" (FontB 11) (Solid $txtPri) 28 447
RRect $g (Solid $chipBg) 28 470 260 26 6
Txt $g "192.168.1.100" (FontN 9) (Solid $txtSec) 36 474
RRect $g (Solid $accent) 298 470 70 26 6
CTxt $g "Connect" (FontN 9) (Solid $wht) 298 470 70 26

# Bottom nav
$g.FillRectangle((Solid $darkBg), 0, 640, 400, 60)
$ni = @("Discover", "History", "EQ", "Settings")
$nx = 20
foreach ($n in $ni) {
    $c = if ($n -eq "Discover") { $accent } else { $txtMut }
    CTxt $g $n (FontN 10) (Solid $c) $nx 648 90 40
    $nx += 90
}

$r.Bmp.Save((Join-Path $screenshotsDir "screenshot-android-discovery.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$r.G.Dispose(); $r.Bmp.Dispose()
Write-Host "  -> android discovery done"

# ============================================================
# 3. Android Playback (400x700)
# ============================================================
Write-Host "Generating Android playback screenshot..."
$r = New-SB -W 400 -H 700
$g = $r.G
$g.FillRectangle((Solid $bg), 0, 0, 400, 700)
$g.FillRectangle((Solid $darkBg), 0, 0, 400, 28)
Txt $g "12:31" (FontN 9) (Solid $txtSec) 16 6
Txt $g "WiFi   BT   [|||]" (FontN 9) (Solid $txtSec) 280 6

Txt $g "LAN Audio" (FontB 18) (Solid $txtPri) 20 45

# Hero orb - streaming (green)
$ox = 140; $oy = 90; $os = 120
$og2 = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.PointF($ox, $oy)),
    (New-Object System.Drawing.PointF(($ox + $os), ($oy + $os))),
    $grn, $teal)
$g.FillEllipse($og2, $ox, $oy, $os, $os); $og2.Dispose()
$g.DrawEllipse((MakePen ([System.Drawing.Color]::FromArgb(50, 34, 197, 94)) 3), ($ox - 8), ($oy - 8), ($os + 16), ($os + 16))
$g.DrawEllipse((MakePen ([System.Drawing.Color]::FromArgb(30, 34, 197, 94)) 2), ($ox - 16), ($oy - 16), ($os + 32), ($os + 32))
CTxt $g "Streaming" (FontB 12) (Solid $grn) $ox ($oy + 42) $os 30
CTxt $g "DESKTOP-PC" (FontN 9) (Solid $txtSec) $ox ($oy + 60) $os 30

# Mode selector
RRect $g (Solid $cardBg) 16 240 368 55
Txt $g "Playback Mode" (FontN 10) (Solid $txtSec) 28 250
$modes = @(
    @{L="Low Latency"; X=28; A=$true},
    @{L="Balanced"; X=144; A=$false},
    @{L="High Quality"; X=260; A=$false}
)
foreach ($m in $modes) {
    $cb = if ($m.A) { (Solid $grn) } else { (Solid $chipBg) }
    $ct = if ($m.A) { $wht } else { $txtSec }
    RRect $g $cb $m.X 268 108 22 6
    CTxt $g $m.L (FontN 8) (Solid $ct) $m.X 268 108 22
}

# Audio Metrics card
RRect $g (Solid $cardBg) 16 308 368 95
Txt $g "Audio Metrics" (FontN 10) (Solid $txtSec) 28 318
$mets = @(
    @{L="Latency(p95)"; V="64ms"; C=$grn},
    @{L="Buffer"; V="2/4 frames"; C=$txtPri},
    @{L="Underruns"; V="0"; C=$grn},
    @{L="Codec"; V="Opus 48kHz"; C=$txtSec}
)
for ($mi = 0; $mi -lt 4; $mi++) {
    $mx = 28 + $mi * 92
    Txt $g $mets[$mi].L (FontN 7) (Solid $txtMut) $mx 342
    Txt $g $mets[$mi].V (FontB 10) (Solid $mets[$mi].C) $mx 356
}

# Jitter card
RRect $g (Solid $cardBg) 16 416 368 100
Txt $g "Jitter Monitor" (FontN 10) (Solid $txtSec) 28 426
$px = 28.0; $pyv = 490.0
for ($i = 1; $i -le 60; $i++) {
    $jx = 28.0 + $i * 5.6
    $jv = [Math]::Abs((Get-Random -Minimum -8 -Maximum 25)) * 1.2
    $jy = 500.0 - $jv
    $jp = if ($jv -lt 15) { (MakePen $grn 1.5) } else { (MakePen $ylw 1.5) }
    $g.DrawLine($jp, [float]$px, [float]$pyv, [float]$jx, [float]$jy)
    $px = $jx; $pyv = $jy
}
$g.DrawLine((MakePen $txtMut 1 $true), 28, 488, 370, 488)
Txt $g "p50=42ms  p95=64ms" (FontN 9) (Solid $txtMut) 28 496

# Volume card
RRect $g (Solid $cardBg) 16 530 368 55
Txt $g "Volume" (FontN 9) (Solid $txtSec) 28 538
RRect $g (Solid $chipBg) 80 545 220 12 6
RRect $g (Solid $accent) 80 545 154 12 6
CTxt $g "70%" (FontN 10) (Solid $txtPri) 310 538 50 24

# Stop button
RRect $g (Solid $red) 140 600 120 40 20
CTxt $g "STOP" (FontB 11) (Solid $wht) 140 600 120 40

# Bottom nav
$g.FillRectangle((Solid $darkBg), 0, 640, 400, 60)
$ni2 = @("Discover", "History", "EQ", "Settings")
$nx2 = 20
foreach ($n in $ni2) {
    $c = if ($n -eq "Discover") { $accent } else { $txtMut }
    CTxt $g $n (FontN 10) (Solid $c) $nx2 648 90 40
    $nx2 += 90
}

$r.Bmp.Save((Join-Path $screenshotsDir "screenshot-android-playback.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$r.G.Dispose(); $r.Bmp.Dispose()
Write-Host "  -> android playback done"

# ============================================================
# 4. Android EQ (400x700)
# ============================================================
Write-Host "Generating Android EQ screenshot..."
$r = New-SB -W 400 -H 700
$g = $r.G
$g.FillRectangle((Solid $bg), 0, 0, 400, 700)
$g.FillRectangle((Solid $darkBg), 0, 0, 400, 28)
Txt $g "12:32" (FontN 9) (Solid $txtSec) 16 6
Txt $g "WiFi   BT   [|||]" (FontN 9) (Solid $txtSec) 280 6

Txt $g "Equalizer" (FontB 18) (Solid $txtPri) 20 45

# Presets card
RRect $g (Solid $cardBg) 16 80 368 60
Txt $g "Presets" (FontB 11) (Solid $txtPri) 28 90
$presets = @("Flat", "Bass+", "Vocal", "Treble", "Custom")
$px = 28
foreach ($pr in $presets) {
    $act = $pr -eq "Custom"
    $pb = if ($act) { (Solid $accent) } else { (Solid $chipBg) }
    $pt = if ($act) { $wht } else { $txtSec }
    RRect $g $pb $px 110 64 22 6
    CTxt $g $pr (FontN 7) (Solid $pt) $px 110 64 22
    $px += 70
}

# Bands card
RRect $g (Solid $cardBg) 16 155 368 220
Txt $g "Bands" (FontB 11) (Solid $txtPri) 28 165
$bands = @(
    @{N="Low"; F="60Hz"; V=4; X=60},
    @{N="Mid"; F="1kHz"; V=-2; X=180},
    @{N="High"; F="8kHz"; V=3; X=300}
)
foreach ($b in $bands) {
    CTxt $g $b.N (FontB 12) (Solid $txtPri) $b.X 195 80 25
    CTxt $g $b.F (FontN 8) (Solid $txtMut) $b.X 218 80 18

    $sx = $b.X + 30; $sy = 245
    RRect $g (Solid $chipBg) $sx $sy 8 100 4

    $slY = $sy + 50 - ($b.V * 5)
    $slH = 50 + ($b.V * 5)
    $sc = if ($b.V -gt 0) { $grn } else { $ylw }
    RRect $g (Solid $sc) $sx $slY 8 $slH 4

    $g.FillEllipse((Solid $wht), ($sx - 4), ($slY - 4), 16, 16)
    $g.FillEllipse((Solid $sc), ($sx - 2), ($slY - 2), 12, 12)

    $vs = if ($b.V -gt 0) { "+$($b.V)dB" } else { "$($b.V)dB" }
    CTxt $g $vs (FontB 10) (Solid $sc) $b.X 348 80 20
}

# Loudness card
RRect $g (Solid $cardBg) 16 390 368 70
Txt $g "Loudness Normalization" (FontB 11) (Solid $txtPri) 28 402
RRect $g (Solid $grn) 310 405 48 26 13
$g.FillEllipse((Solid $wht), 334, 407, 22, 22)
Txt $g "Gain: +2.4 dB" (FontN 9) (Solid $txtSec) 28 430

# Bottom nav
$g.FillRectangle((Solid $darkBg), 0, 640, 400, 60)
$ni3 = @("Discover", "History", "EQ", "Settings")
$nx3 = 20
foreach ($n in $ni3) {
    $c = if ($n -eq "EQ") { $accent } else { $txtMut }
    CTxt $g $n (FontN 10) (Solid $c) $nx3 648 90 40
    $nx3 += 90
}

$r.Bmp.Save((Join-Path $screenshotsDir "screenshot-android-eq.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$r.G.Dispose(); $r.Bmp.Dispose()
Write-Host "  -> android EQ done"

# ============================================================
# 5. Architecture Diagram (700x380)
# ============================================================
Write-Host "Generating architecture diagram..."
$r = New-SB -W 700 -H 380
$g = $r.G
$g.FillRectangle((Solid $bg), 0, 0, 700, 380)

Txt $g "System Architecture" (FontB 14) (Solid $txtPri) 240 16

# Windows box
RRect $g (Solid $cardBg) 20 55 310 300
Txt $g "Windows (Sender)" (FontB 12) (Solid $accent) 36 68

$wbs = @(
    @{N="Tauri Desktop App"; D="System tray, snapshot UI"; Y=98},
    @{N="lan_audio_server"; D="Capture, encode, stream, session mgmt"; Y=148},
    @{N="WASAPI Loopback"; D="System audio capture"; Y=198},
    @{N="Opus Encoder"; D="Low-latency Opus encoding"; Y=248}
)
foreach ($wb in $wbs) {
    RRect $g (Solid $chipBg) 40 $wb.Y 270 40 6
    Txt $g $wb.N (FontN 10) (Solid $txtPri) 52 ($wb.Y + 4)
    Txt $g $wb.D (FontN 8) (Solid $txtMut) 52 ($wb.Y + 22)
}

# Android box
RRect $g (Solid $cardBg) 370 55 310 300
Txt $g "Android (Receiver)" (FontB 12) (Solid $grn) 386 68

$abs = @(
    @{N="Flutter UI (Audio Console Dark)"; D="Discovery, playback, EQ, settings"; Y=98},
    @{N="Playback Service (Foreground)"; D="MediaSession, notification, keepalive"; Y=148},
    @{N="Opus Decoder (JNI)"; D="Hardware-accelerated decode"; Y=198},
    @{N="Oboe / AudioTrack"; D="Low-latency audio output"; Y=248}
)
foreach ($ab in $abs) {
    RRect $g (Solid $chipBg) 390 $ab.Y 270 40 6
    Txt $g $ab.N (FontN 10) (Solid $txtPri) 402 ($ab.Y + 4)
    Txt $g $ab.D (FontN 8) (Solid $txtMut) 402 ($ab.Y + 22)
}

# Arrows and protocol info
$ar = MakePen $accent 3
$ar.EndCap = [System.Drawing.Drawing2D.LineCap]::ArrowAnchor
$g.DrawLine($ar, 340, 200, 360, 200)
Txt $g "WiFi / USB" (FontN 8) (Solid $txtSec) 324 210
$ar.EndCap = [System.Drawing.Drawing2D.LineCap]::ArrowAnchor
$g.DrawLine($ar, 360, 310, 340, 310)

Txt $g "Protocol v2 (Opus)" (FontN 8) (Solid $accent) 130 310
Txt $g "Legacy fallback (PCM16)" (FontN 8) (Solid $txtMut) 130 328

Txt $g "Reverse Audio Channel" (FontB 9) (Solid $ylw) 470 310
Txt $g "Mic -> PC (Opus TCP :7878)" (FontN 8) (Solid $txtSec) 470 328
Txt $g "Volume Control TCP :7879" (FontN 8) (Solid $txtSec) 470 344

$r.Bmp.Save((Join-Path $screenshotsDir "screenshot-architecture.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$r.G.Dispose(); $r.Bmp.Dispose()
Write-Host "  -> architecture done"

Write-Host ""
Write-Host "All screenshots generated in: $screenshotsDir"
Get-ChildItem $screenshotsDir | ForEach-Object {
    Write-Host "  $($_.Name) ($([math]::Round($_.Length/1KB, 1)) KB)"
}
