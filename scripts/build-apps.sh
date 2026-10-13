#!/bin/bash
# Builds Canon Webcam.app, tucks it inside "Install Canon Webcam", adds "Uninstall Canon Webcam",
# and wraps the installer and uninstaller in a disk image.
set -euo pipefail
cd "$(dirname "$0")/.."

# Bump this for each release, it names the disk image and goes into every app's Info.plist
VERSION="1.1.1"
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
    local folder="$1" name="$2" executable="$3" identifier="$4" icon="$5" camera_reason="$6" menu_bar_only="$7"
    local app="$folder/$name.app"
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
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>2</string>
    <key>LSMinimumSystemVersion</key><string>26.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSCameraUsageDescription</key><string>$camera_reason</string>
    <key>LSUIElement</key><$menu_bar_only/>
    <key>NSHumanReadableCopyright</key><string>Made by Cristopher Cervantes. Not affiliated with or endorsed by Canon Inc.</string>
</dict>
</plist>
PLIST
}

echo "Drawing icons"
make_icns app
make_icns installer
make_icns uninstaller

echo "Assembling Canon Webcam.app"
# Menu bar only by default, it shows a Dock icon by itself while its window is open
make_app "$WORK" "Canon Webcam" CanonWebcam dev.canon-open-webcam.app app \
    "Shows you exactly what your camera sends to Zoom, Meet and FaceTime." true
cp "$RELEASE/canon-webcam" "$WORK/Canon Webcam.app/Contents/Resources/canon-webcam"
codesign --force --deep --sign - "$WORK/Canon Webcam.app"

echo "Assembling the installer and uninstaller"
make_app "$STAGE" "Install Canon Webcam" CanonWebcamInstaller dev.canon-open-webcam.installer installer \
    "Not used, the installer never opens the camera." false
make_app "$STAGE" "Uninstall Canon Webcam" CanonWebcamUninstaller dev.canon-open-webcam.uninstaller uninstaller \
    "Not used, the uninstaller never opens the camera." false
# The installer carries the whole app, so it works on a Mac without this repo
cp -R "$WORK/Canon Webcam.app" "$STAGE/Install Canon Webcam.app/Contents/Resources/"

echo "Signing for this Mac"
for app in "$STAGE"/*.app; do
    codesign --force --deep --sign - "$app"
done

echo "Making the disk image"
mv "$WORK/Canon Webcam.app" "$DIST/Canon Webcam.app"
rm -rf "$WORK"
# No spaces in the file name, GitHub would turn them into dots
hdiutil create -quiet -volname "Canon Webcam $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DIST/CanonWebcam-$VERSION.dmg"

echo "Done: $DIST/CanonWebcam-$VERSION.dmg (and $DIST/Canon Webcam.app for a quick look)"
