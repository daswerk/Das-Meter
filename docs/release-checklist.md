# Release checklist

Steps a release needs besides a green CI. Each blocks the release if it fails.

## Performance budget

Run on both reference machines, each on its built-in display, on mains power,
with nothing else busy:

- a base **M1 MacBook Air** (8 GB);
- an **i5-1135G7 / Ryzen 5 5500U-class** laptop with integrated graphics.

```sh
cargo build --release -p dasmeter-app
./target/release/das-meter --benchmark both
```

It runs the Busy and Silent scenes three times each (5 s warm-up, 60 s
measured) and prints the best run of each. While it runs, keep Activity
Monitor ▸ Window ▸ GPU History open for the GPU figure, which the benchmark
can't read itself.

Compare with the budget ([Set the performance budget](https://github.com/daswerk/Das-Meter/issues/10)):

| Busy scene                | M1 MacBook Air | Windows iGPU laptop |
|---------------------------|----------------|---------------------|
| App CPU (% of one core)   | ≤ 10 %         | ≤ 15 %              |
| GPU                       | ≤ 10 %         | ≤ 15 %              |
| Resident memory           | ≤ 150 MB       | ≤ 150 MB            |
| Late frames (> 16.7 ms)   | ≤ 1 %, no stall over 50 ms after warm-up | same |

- **Silent scene**: CPU ≤ 1 % of a core, and no frames drawn.
- **Latency** (click → screen, from the test signal's clicks): about 2 frames (~35 ms).

A miss blocks the release: note the numbers in the release issue and fix, or
reopen the budget issue with the measurements.

## Loudness conformance

Run the EBU test set at 48 kHz (`--features ebu-conformance`), per
[ADR 0005](adr/0005-measurement-definitions-and-verification.md). The EBU
files are downloaded by hand and never committed.

## One-time setup: signing (ADR 0004)

Done once by the owner; the release workflow (`.github/workflows/release.yml`)
reads the results from the `release` environment.

1. **Environment**: Settings ▸ Environments ▸ New environment `release`. Add
   yourself under Required reviewers, and limit it to `v*` tags.
2. **Code-signing certificate** (self-made, so the System Capture permission
   survives updates): Keychain Access ▸ Certificate Assistant ▸ Create a
   Certificate… Name "Das-Meter", Identity Type *Self-Signed Root*,
   Certificate Type *Code Signing*, and tick "Let me override defaults" to set
   the validity to 20 years (7300 days). Then export it with its key as a
   `.p12` with a password.
   - Secret `MACOS_CERTIFICATE`: `base64 -i Das-Meter.p12 | pbcopy`
   - Secret `MACOS_CERTIFICATE_PASSWORD`: the export password.
   - Variable `CERTIFICATE_SHA1`: the certificate's SHA-1, without spaces:
     `security find-certificate -c Das-Meter -Z | awk '/SHA-1/ { print $3 }'`
   Keep the `.p12` somewhere safe: a new certificate means every user grants
   System Capture again, and existing installs refuse its updates.
3. **Update signing key**: `brew install minisign`, then
   `minisign -G -W -p das-meter.pub -s das-meter.key` (no password, so CI can
   sign).
   - Secret `MINISIGN_SECRET_KEY`: the whole text of `das-meter.key`.
   - Variable `MINISIGN_PUBLIC_KEY`: the second line of `das-meter.pub` (the
     base64 key).
   Keep `das-meter.key` safe and offline; losing it means users update by hand.
4. **Developer ID (later)**: once there's an Apple Developer account, add
   `APPLE_DEVELOPER_ID_CERTIFICATE`, `APPLE_DEVELOPER_ID_PASSWORD`, `APPLE_ID`,
   `APPLE_TEAM_ID` and `APPLE_APP_PASSWORD`. The workflow then signs with the
   Developer ID and notarizes, and the DMG no longer needs Open Anyway. Note
   that users of self-signed builds must then grant System Capture once more.

## Making a release

1. Bump `version` in the workspace `Cargo.toml`, merge to `main`.
2. `git tag v<version> && git push origin v<version>`; approve the `release`
   environment run.
3. Check the draft Release: the DMG, `Das-Meter-<version>.app.tar.gz` and its
   `.minisig`. Install from the DMG on a clean Mac, and check that an
   installed older build offers the update and keeps its System Capture
   permission. Then publish the draft: apps find it within a day.

A tag with a suffix (`v0.2.0-test1`) makes a pre-release, which apps never
offer as an update: use it for dry runs, then delete the draft and the tag.
