#!/usr/bin/env bash
# Makes the macOS DMG (ADR 0004): only Das-Meter.app and an Applications
# link, over a background that shows drag-to-Applications and the Open
# Anyway steps.
#
#   scripts/macos-dmg.sh APP OUT.dmg
#
# Finder lays the window out through AppleScript, so run it in a logged-in
# session (GitHub's macOS runners are). Set IDENTITY to sign the DMG.
set -euo pipefail

app=$1
dmg=$2
root=$(cd "$(dirname "$0")/.." && pwd)
volume="Das-Meter"
work=$(mktemp -d)
mount=""
cleanup() {
    if [[ -n $mount ]]; then hdiutil detach "$mount" -quiet 2>/dev/null || true; fi
    rm -rf "$work"
}
trap cleanup EXIT

mkdir -p "$work/stage/.background"
cp -R "$app" "$work/stage/"
ln -s /Applications "$work/stage/Applications"
swift "$root/scripts/dmg-background.swift" "$work/bg1.png" 1
swift "$root/scripts/dmg-background.swift" "$work/bg2.png" 2
# One multi-resolution TIFF, so the background is sharp on Retina.
tiffutil -cathidpicheck "$work/bg1.png" "$work/bg2.png" -out "$work/stage/.background/background.tiff" 2>/dev/null

hdiutil create -quiet -srcfolder "$work/stage" -volname "$volume" -fs HFS+ \
    -format UDRW -ov "$work/rw.dmg"
# Mounted under /Volumes, where Finder sees it; its name may get a suffix if
# another "Das-Meter" volume is mounted.
mount=$(hdiutil attach -readwrite -noverify -noautoopen "$work/rw.dmg" |
    awk -F'\t' '/\/Volumes\// { print $NF; exit }')
disk_name=$(basename "$mount")

app_name=$(basename "$app")
# The window layout is a nicety: if Finder can't do it (no GUI session), the
# DMG is still made, with Finder's default layout.
osascript <<APPLESCRIPT || echo "macos-dmg.sh: warning: Finder couldn't lay out the window" >&2
tell application "Finder"
    set theDisk to disk "$disk_name"
    open theDisk
    set theWindow to container window of theDisk
    set current view of theWindow to icon view
    set toolbar visible of theWindow to false
    set statusbar visible of theWindow to false
    set the bounds of theWindow to {200, 120, 860, 560}
    set viewOptions to the icon view options of theWindow
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 96
    set text size of viewOptions to 13
    set background picture of viewOptions to file ".background:background.tiff" of theDisk
    set position of item "$app_name" of theDisk to {170, 150}
    set position of item "Applications" of theDisk to {490, 150}
    update theDisk without registering applications
    delay 1
    close theWindow
end tell
APPLESCRIPT
sync
hdiutil detach -quiet "$mount"
mount=""

rm -f "$dmg"
hdiutil convert -quiet "$work/rw.dmg" -format UDZO -imagekey zlib-level=9 -o "$dmg"
if [[ -n ${IDENTITY:-} ]]; then
    codesign --force --sign "$IDENTITY" "$dmg"
fi
echo "$dmg"
