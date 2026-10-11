# Builds a release and zips the two files a person needs: the app and its camera DLL.
#   powershell -ExecutionPolicy Bypass -File scripts\package.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
    $name = "CanonWebcam-$version-windows-x64"
    $stage = Join-Path "dist" $name
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force $stage | Out-Null

    Copy-Item target\release\CanonWebcam.exe $stage
    Copy-Item target\release\canon_vcam.dll $stage
    @"
Canon Webcam $version

Double click CanonWebcam.exe and say yes to install. Windows asks for permission once.
Then pick "Canon Webcam" as the camera in Zoom, Teams, Meet or any other app.

Needs Windows 11. Keep CanonWebcam.exe and canon_vcam.dll together until it's installed.
To remove it: Settings > Apps > Installed apps > Canon Webcam, or the tray icon's menu.
"@ | Set-Content -Encoding utf8 (Join-Path $stage "README.txt")

    $zip = Join-Path "dist" "$name.zip"
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path "$stage\*" -DestinationPath $zip
    Write-Host "Made $zip"
}
finally {
    Pop-Location
}
