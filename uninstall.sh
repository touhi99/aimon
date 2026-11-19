#!/bin/zsh
launchctl bootout gui/$(id -u)/dev.aimon.app 2>/dev/null || true
rm -f ~/Library/LaunchAgents/dev.aimon.app.plist
echo "AI Monitor stopped. Delete ~/Library/Application Support/aimon to remove its data too."
