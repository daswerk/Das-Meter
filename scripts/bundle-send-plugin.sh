#!/usr/bin/env bash
# Builds the Send Plugin and bundles it as CLAP and VST3 (macOS and Windows)
# and AUv2 (macOS). The one library holds all three entry points: the CLAP
# entry, and the VST3 and AU entries clap-wrapper-rs adds (ADR 0001).
#
#   scripts/bundle-send-plugin.sh [OUT_DIR]      # default target/plugins
#
# On macOS the bundles are signed ad hoc; set IDENTITY to sign with a certificate.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$root/target/plugins}
name="Das-Meter Send"
bundle_id=com.daswerk.das-meter.send

cargo build --release --locked -p dasmeter-send --manifest-path "$root/Cargo.toml"
version=$(cargo pkgid -p dasmeter-send --manifest-path "$root/Cargo.toml" | sed 's/.*[#@]//')

rm -rf "$out"
mkdir -p "$out"

case "$(uname -s)" in
Darwin)
    lib="$root/target/release/libdasmeter_send.dylib"
    # AU versions are one integer: major << 16 | minor << 8 | patch.
    IFS=. read -r major minor patch <<<"$version"
    au_version=$(((major << 16) | (minor << 8) | patch))

    # clap-wrapper's AU view classes are Objective-C classes named with a
    # build-time number, so the CLAP, VST3 and AU copies of the one library
    # define the same classes. Hosts that load several formats into one
    # process (REAPER does) then resolve the AU's view class to the CLAP's copy,
    # and the AU opens without its window. Rename them
    # in the AU copy, keeping the length (the digits each shift by one).
    unique_au_view_classes() { # unique_au_view_classes BINARY
        local number renamed
        # sort -u, not head: head would end the pipe early, and pipefail would
        # take the SIGPIPE for a failure.
        number=$(strings -a "$1" | sed -n 's/^wrapAsAUV2_cocoaUI_\([0-9][0-9]*\)$/\1/p' | sort -u)
        if [[ -z $number || $number == *$'\n'* ]]; then
            echo "bundle-send-plugin.sh: expected one AU view class to rename, found: ${number:-none}" >&2
            exit 1
        fi
        renamed=$(tr 0-9 1-90 <<<"$number")
        perl -0777 -pi -e "s/(wrapAsAUV2_cocoaUI_(?:nsview_)?)$number/\${1}$renamed/g" "$1"
    }

    bundle() { # bundle EXTENSION PACKAGE_TYPE [EXTRA_PLIST_XML]
        local dir="$out/$name.$1"
        mkdir -p "$dir/Contents/MacOS"
        cp "$lib" "$dir/Contents/MacOS/$name"
        if [[ $1 == component ]]; then
            unique_au_view_classes "$dir/Contents/MacOS/$name"
        fi
        printf '%s????' "$2" >"$dir/Contents/PkgInfo"
        cat >"$dir/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>English</string>
    <key>CFBundleExecutable</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$bundle_id.$1</string>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>$2</string>
    <key>CFBundleSignature</key><string>????</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>14.6</string>
    <key>NSHumanReadableCopyright</key><string>Das Werk, MIT OR Apache-2.0</string>${3:-}
</dict>
</plist>
PLIST
        codesign --force --sign "${IDENTITY:--}" "$dir"
    }

    bundle clap BNDL
    bundle vst3 BNDL
    bundle component BNDL "
    <key>AudioComponents</key>
    <array>
        <dict>
            <key>type</key><string>aufx</string>
            <key>subtype</key><string>DmSd</string>
            <key>manufacturer</key><string>DsWk</string>
            <key>name</key><string>Das Werk: $name</string>
            <key>description</key><string>Sends this track's audio to the Das-Meter app</string>
            <key>version</key><integer>$au_version</integer>
            <key>factoryFunction</key><string>GetPluginFactoryAUV2</string>
            <key>sandboxSafe</key><true/>
            <key>tags</key><array><string>Effects</string></array>
        </dict>
    </array>"
    ;;
MINGW* | MSYS* | CYGWIN*)
    dll="$root/target/release/dasmeter_send.dll"
    cp "$dll" "$out/$name.clap"
    mkdir -p "$out/$name.vst3/Contents/x86_64-win"
    cp "$dll" "$out/$name.vst3/Contents/x86_64-win/$name.vst3"
    ;;
*)
    echo "bundle-send-plugin.sh: only macOS and Windows are supported" >&2
    exit 1
    ;;
esac

echo "Bundles in $out:"
ls "$out"
