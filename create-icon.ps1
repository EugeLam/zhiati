Add-Type -AssemblyName System.Drawing

$iconPath = "D:/lyj/code/memo/zhiati/win-desktop/icons"

# Create 32x32 bitmap
$bmp32 = New-Object System.Drawing.Bitmap(32, 32)
$g32 = [System.Drawing.Graphics]::FromImage($bmp32)
$yellow32 = [System.Drawing.Color]::FromArgb(255, 255, 200, 0)
$g32.Clear($yellow32)
$g32.Dispose()
$bmp32.Save("$iconPath/32x32.png", [System.Drawing.Imaging.ImageFormat]::Png)

# Create 128x128 bitmap
$bmp128 = New-Object System.Drawing.Bitmap(128, 128)
$g128 = [System.Drawing.Graphics]::FromImage($bmp128)
$g128.Clear($yellow32)
$g128.Dispose()
$bmp128.Save("$iconPath/128x128.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp128.Save("$iconPath/128x128@2x.png", [System.Drawing.Imaging.ImageFormat]::Png)

# Save icon.png (32x32)
$bmp32.Save("$iconPath/icon.png", [System.Drawing.Imaging.ImageFormat]::Png)

# Create proper ICO file with multiple sizes
$icoPath = "$iconPath/icon.ico"
$bmp256 = New-Object System.Drawing.Bitmap(256, 256)
$g256 = [System.Drawing.Graphics]::FromImage($bmp256)
$g256.Clear($yellow32)
$g256.Dispose()

# Save as ICO using Icon class
$hIcon = $bmp256.GetHicon()
$icon = [System.Drawing.Icon]::FromHandle($hIcon)
$fs = [System.IO.FileStream]::new($icoPath, [System.IO.FileMode]::Create)
$icon.Save($fs)
$fs.Close()
$fs.Dispose()

# Clean up
$bmp32.Dispose()
$bmp128.Dispose()
$bmp256.Dispose()

Write-Host "Icons created successfully"
