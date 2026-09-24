#!/usr/bin/env bash
# Fails if the headless crates (app core, Meter analysis) depend on platform,
# GPU or audio-device crates, on any target. They must stay testable without
# windows, a GPU or audio devices; that code belongs in dasmeter-app.
set -euo pipefail

headless=(dasmeter-core dasmeter-analysis)
banned=(
    winit wgpu egui egui-wgpu egui-winit raw-window-handle baseview
    cpal coreaudio-rs coreaudio-sys wasapi
    objc2 cocoa windows
    clack-plugin clack-host
)

status=0
for crate in "${headless[@]}"; do
    deps=$(cargo tree --package "$crate" --edges normal,build --target all \
        --prefix none --format '{p}' | awk '{print $1}' | sort -u)
    for name in "${banned[@]}"; do
        if grep -qx "$name" <<<"$deps"; then
            echo "error: $crate depends on $name, which is platform, GPU or audio-device code" >&2
            status=1
        fi
    done
done

if [[ $status -eq 0 ]]; then
    echo "ok: ${headless[*]} have no platform, GPU or audio-device dependencies"
fi
exit $status
