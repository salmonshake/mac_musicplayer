#!/bin/bash
# Builds a release Maplay.app into dist/. With --install, also copies it to /Applications.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -n 1)
APP=dist/Maplay.app

cargo build --release -p mmp-app

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/maplay "$APP/Contents/MacOS/maplay"
cp app/assets/Maplay.icns "$APP/Contents/Resources/Maplay.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Maplay</string>
    <key>CFBundleDisplayName</key><string>Maplay</string>
    <key>CFBundleIdentifier</key><string>io.github.salmonshake.maplay</string>
    <key>CFBundleExecutable</key><string>maplay</string>
    <key>CFBundleIconFile</key><string>Maplay</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# Ad-hoc signature (enough to run on this Mac; distribution needs a Developer ID).
codesign --force --deep --sign - "$APP"
echo "Built $APP ($VERSION)"

if [ "${1:-}" = "--install" ]; then
    rm -rf /Applications/Maplay.app
    cp -R "$APP" /Applications/
    echo "Installed /Applications/Maplay.app"
fi
