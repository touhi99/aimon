#!/bin/zsh
# Stops AI Project Monitor and removes the login item. Data in ~/Library/Application Support/aimon is kept.
launchctl bootout gui/$(id -u)/dev.aimon.app 2>/dev/null || true
rm -f ~/Library/LaunchAgents/dev.aimon.app.plist
echo "AI Project Monitor stopped. Delete ~/Library/Application Support/aimon to remove its data too."
