#!/usr/bin/env bash
# Builds Das-Meter.app for running on this Mac: the release binary in a bundle
# with the Info.plist keys System Capture needs, signed with the Hardened
# Runtime. Ad hoc by default; set IDENTITY to sign with a certificate.
#
#   scripts/macos-bundle.sh [--audio-input] [--bundle-id ID] [--name NAME] [OUT_DIR]
#
# --audio-input adds the com.apple.security.device.audio-input entitlement.
# The capture permission is tied to the signature: an ad hoc signed build is
# asked again after every rebuild.
set -euo pipefail

audio_input=false
bundle_id=com.daswerk.das-meter
name=Das-Meter
while [[ $# -gt 0 ]]; do
    case $1 in
        --audio-input) audio_input=true; shift ;;
        --bundle-id) bundle_id=$2; shift 2 ;;
        --name) name=$2; shift 2 ;;
        *) break ;;
    esac
done
out=${1:-target/bundle}

root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --release --locked -p dasmeter-app --manifest-path "$root/Cargo.toml"
version=$(cargo pkgid -p dasmeter-app --manifest-path "$root/Cargo.toml" | sed 's/.*[#@]//')

app="$out/$name.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$root/target/release/das-meter" "$app/Contents/MacOS/das-meter"

cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleExecutable</key><string>das-meter</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>14.6</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSAudioCaptureUsageDescription</key>
    <string>Das-Meter shows meters for the audio your Mac plays.</string>
</dict>
</plist>
PLIST

entitlements=$(mktemp)
trap 'rm -f "$entitlements"' EXIT
{
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
    echo '<plist version="1.0"><dict>'
    if $audio_input; then
        echo '<key>com.apple.security.device.audio-input</key><true/>'
    fi
    echo '</dict></plist>'
} >"$entitlements"

codesign --force --options runtime --entitlements "$entitlements" \
    --sign "${IDENTITY:--}" "$app"
codesign --verify --strict "$app"
echo "$app"
