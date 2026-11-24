#!/bin/zsh
set -e
cd "$(dirname "$0")"
[ -d AIMon.app ] || { echo "AIMon.app not found. Run ./build.sh first."; exit 1; }
DEST=~/Library/LaunchAgents/dev.aimon.app.plist
mkdir -p ~/Library/LaunchAgents ~/Library/Logs/dev
sed -e "s#__AIMON_DIR__#$PWD#" -e "s#__LOG__#$HOME/Library/Logs/dev/dev.aimon.app.log#" dev.aimon.app.plist.template > "$DEST"
launchctl bootout gui/$(id -u)/dev.aimon.app 2>/dev/null || true
launchctl bootstrap gui/$(id -u) "$DEST"
echo "AI Monitor is running and will start at login."
