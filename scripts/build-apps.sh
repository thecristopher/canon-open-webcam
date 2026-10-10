#!/bin/bash
# Builds "Install Canon Webcam" and "Uninstall Canon Webcam", then wraps both in a disk image.
set -euo pipefail
cd "$(dirname "$0")/.."

DIST="dist"
STAGE="$DIST/Canon Webcam"
WORK="$DIST/.work"
RELEASE=".build/release"

echo "Building release binaries"
swift build -c release

rm -rf "$DIST"
mkdir -p "$STAGE" "$WORK"

# Turns one 1024px PNG into the .icns file a Mac app bundle expects
make_icns() {
    local kind="$1" iconset="$WORK/$1.iconset"
    mkdir -p "$iconset"
    swift scripts/make-icon.swift "$kind" "$WORK/$kind.png"
    for size in 16 32 128 256 512; do
        sips -z $size $size "$WORK/$kind.png" --out "$iconset/icon_${size}x${size}.png" > /dev/null
        sips -z $((size * 2)) $((size * 2)) "$WORK/$kind.png" --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
    done
    iconutil -c icns "$iconset" -o "$WORK/$kind.icns"
}

# Wraps a SwiftPM executable in a double clickable .app bundle
make_app() {
    local name="$1" executable="$2" identifier="$3" icon="$4" camera_reason="$5"
    local app="$STAGE/$name.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$RELEASE/$executable" "$app/Contents/MacOS/$executable"
    cp "$WORK/$icon.icns" "$app/Contents/Resources/AppIcon.icns"
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$executable</string>
    <key>CFBundleIdentifier</key><string>$identifier</string>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>26.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSCameraUsageDescription</key><string>$camera_reason</string>
</dict>
</plist>
PLIST
}

echo "Drawing icons"
make_icns installer
make_icns uninstaller

echo "Assembling apps"
make_app "Install Canon Webcam" CanonWebcamInstaller dev.canon-open-webcam.installer installer \
    "Shows a live preview so you can check your Canon is working."
make_app "Uninstall Canon Webcam" CanonWebcamUninstaller dev.canon-open-webcam.uninstaller uninstaller \
    "Not used, the uninstaller never opens the camera."
# The installer carries the engine, so it works on a Mac without this repo
cp "$RELEASE/canon-webcam" "$STAGE/Install Canon Webcam.app/Contents/Resources/canon-webcam"

echo "Signing for this Mac"
for app in "$STAGE"/*.app; do
    codesign --force --deep --sign - "$app"
done

echo "Making the disk image"
rm -rf "$WORK"
hdiutil create -quiet -volname "Canon Webcam" -srcfolder "$STAGE" -ov -format UDZO "$DIST/Canon Webcam.dmg"

echo "Done: $DIST/Canon Webcam.dmg"
