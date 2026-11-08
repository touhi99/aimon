#!/bin/zsh
# Builds the Rust core and the Swift menu bar app, bundles them into AIMon.app
set -e
cd "$(dirname "$0")"
cargo build --release
(cd app && swift build -c release)
APP=AIMon.app
rm -rf $APP && mkdir -p $APP/Contents/MacOS $APP/Contents/Resources
cp app/.build/release/AIMon $APP/Contents/MacOS/AIMon
cp target/release/aimon $APP/Contents/Resources/aimon
cat > $APP/Contents/Info.plist <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>AI Project Monitor</string>
<key>CFBundleIdentifier</key><string>dev.aimon.app</string>
<key>CFBundleExecutable</key><string>AIMon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1</string>
<key>LSUIElement</key><true/>
</dict></plist>
P
codesign --force --sign - $APP 2>/dev/null || true
echo "built $APP"
