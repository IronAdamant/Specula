param([string]$dir)
# Windows GDI+ (System.Drawing) as the external JPEG encoder and decoder for the known-answer fixtures.
Add-Type -AssemblyName System.Drawing
$src = New-Object System.Drawing.Bitmap((Join-Path $dir "kat_source.png"))
$codec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq "image/jpeg" }
$params = New-Object System.Drawing.Imaging.EncoderParameters(1)
$params.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter([System.Drawing.Imaging.Encoder]::Quality, [long]85)
$src.Save((Join-Path $dir "kat_gdi.jpg"), $codec, $params)
$src.Dispose()
foreach ($pair in @(@("kat_gdi.jpg", "kat_gdi_decoded.png"), @("kat_specula.jpg", "kat_specula_gdi_decoded.png"))) {
  $in = Join-Path $dir $pair[0]
  if (Test-Path $in) {
    $b = New-Object System.Drawing.Bitmap($in)
    $rect = New-Object System.Drawing.Rectangle(0, 0, $b.Width, $b.Height)
    $c = $b.Clone($rect, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $c.Save((Join-Path $dir $pair[1]), [System.Drawing.Imaging.ImageFormat]::Png)
    $c.Dispose(); $b.Dispose()
    "wrote " + $pair[1]
  }
}
