#!/usr/bin/env bash
# Builds Das-Meter.app: the release binary in a bundle with the Info.plist keys
# System Capture needs, the Send Plugin (CLAP, VST3, AU) inside it for the app
# to install (ADR 0004), and `.dasmeter-preset` registered so Finder opens
# Presets with it. Signed with the Hardened Runtime: ad hoc by default; set
# IDENTITY to sign with a certificate.
#
#   scripts/macos-bundle.sh [--universal] [--audio-input] [--bundle-id ID] [--name NAME] [OUT_DIR]
#
# --universal builds for arm64 and x86_64 (releases do).
# --audio-input adds the com.apple.security.device.audio-input entitlement.
# The capture permission is tied to the signature: an ad hoc signed build is
# asked again after every rebuild.
#
# For a release, set DASMETER_MINISIGN_PUBLIC_KEY and DASMETER_CERTIFICATE_SHA1
# so the app can verify and install its updates.
set -euo pipefail

audio_input=false
universal=false
bundle_id=com.daswerk.das-meter
name=Das-Meter
while [[ $# -gt 0 ]]; do
    case $1 in
        --audio-input) audio_input=true; shift ;;
        --universal) universal=true; shift ;;
        --bundle-id) bundle_id=$2; shift 2 ;;
        --name) name=$2; shift 2 ;;
        *) break ;;
    esac
done
out=${1:-target/bundle}

root=$(cd "$(dirname "$0")/.." && pwd)
if $universal; then
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release --locked -p dasmeter-app --target "$target" --manifest-path "$root/Cargo.toml"
    done
    mkdir -p "$root/target/universal"
    binary="$root/target/universal/das-meter"
    lipo -create -output "$binary" \
        "$root/target/aarch64-apple-darwin/release/das-meter" \
        "$root/target/x86_64-apple-darwin/release/das-meter"
    plugin_flags=(--universal)
else
    cargo build --release --locked -p dasmeter-app --manifest-path "$root/Cargo.toml"
    binary="$root/target/release/das-meter"
    plugin_flags=()
fi
version=$(cargo pkgid -p dasmeter-app --manifest-path "$root/Cargo.toml" | sed 's/.*[#@]//')
plugins="$root/target/bundle-plugins"
"$root/scripts/bundle-send-plugin.sh" "${plugin_flags[@]+"${plugin_flags[@]}"}" "$plugins" >/dev/null

app="$out/$name.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/PlugIns"
cp "$binary" "$app/Contents/MacOS/das-meter"
# The Send Plugin bundles, signed already, as the app installs them.
cp -R "$plugins/" "$app/Contents/PlugIns/"

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
    <key>NSHumanReadableCopyright</key><string>Das Werk, MIT OR Apache-2.0</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>Das-Meter Preset</string>
            <key>CFBundleTypeRole</key><string>Viewer</string>
            <key>LSHandlerRank</key><string>Owner</string>
            <key>LSItemContentTypes</key>
            <array><string>com.daswerk.das-meter.preset</string></array>
        </dict>
    </array>
    <key>UTExportedTypeDeclarations</key>
    <array>
        <dict>
            <key>UTTypeIdentifier</key><string>com.daswerk.das-meter.preset</string>
            <key>UTTypeDescription</key><string>Das-Meter Preset</string>
            <key>UTTypeConformsTo</key>
            <array><string>public.data</string><string>public.content</string></array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array><string>dasmeter-preset</string></array>
            </dict>
        </dict>
    </array>
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

xattr -cr "$app"
codesign --force --options runtime --entitlements "$entitlements" \
    --sign "${IDENTITY:--}" "$app"
codesign --verify --strict "$app"
echo "$app"
