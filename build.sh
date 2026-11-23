#!/bin/zsh
set -e
cd "$(dirname "$0")"
cargo build --release
(cd app && swift build -c release)
APP=AIMon.app
rm -rf $APP && mkdir -p $APP/Contents/MacOS $APP/Contents/Resources
cp app/.build/release/AIMon $APP/Contents/MacOS/AIMon
cp target/release/aimon $APP/Contents/Resources/aimon
ICON=app/.build/AppIcon.iconset
rm -rf $ICON && mkdir -p $ICON
AIMON_ICON=app/.build/icon-1024.png app/.build/release/AIMon
for s in 16 32 128 256 512; do
  sips -z $s $s app/.build/icon-1024.png --out $ICON/icon_${s}x${s}.png >/dev/null
  sips -z $((s*2)) $((s*2)) app/.build/icon-1024.png --out $ICON/icon_${s}x${s}@2x.png >/dev/null
done
iconutil -c icns $ICON -o $APP/Contents/Resources/AppIcon.icns
cat > $APP/Contents/Info.plist <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>AI Monitor</string>
<key>CFBundleIdentifier</key><string>dev.aimon.app</string>
<key>CFBundleExecutable</key><string>AIMon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundleVersion</key><string>0.1</string>
<key>LSUIElement</key><true/>
</dict></plist>
P
codesign --force --sign - $APP 2>/dev/null || true
echo "built $APP"
