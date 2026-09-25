# macOS ships without a Developer ID, and the app installs its own Send Plugin

For now Das-Meter is released on macOS without the Apple Developer Program (99 USD/yr), so nothing is notarized. Without notarization, Gatekeeper needs one **Open Anyway** in System Settings for each downloaded item, and DAWs ask before loading quarantined, unnotarized plug-ins. So macOS ships as **one DMG holding only the app**. The app carries the Send Plugin (CLAP, VST3, AU) inside its bundle and **copies it itself** into the per-user folders (`~/Library/Audio/Plug-Ins/{CLAP,VST3,Components}`). That needs no admin password, and files the app copies itself don't carry the quarantine flag, so the user sees one Gatekeeper step instead of several.

We rejected an unsigned `.pkg` installing system-wide (as MiniMeters and Surge XT do with a notarized one): the installer itself would need Open Anyway, an admin password, and the plug-ins could still be quarantined. Windows is unaffected and keeps a signed, machine-wide Inno Setup installer.

## Consequences

- **Identity**: every release is signed with one self-made code-signing certificate, not ad hoc, so the System Capture permission (tied to the code signature) should survive updates. This is to be verified with two builds.
- **Updates** bypass Gatekeeper, so the app checks them itself. It downloads a `.app.tar.gz`, verifies a minisign (Ed25519) signature against a built-in public key and checks that the app is signed with our certificate, then swaps itself in by rename. At each launch, it replaces any installed Send Plugin format that is older than the bundled one, by writing a temp file and renaming it over the old one (never in place).
- **Uninstall** is a menu item in the app, since there's no `.pkg` receipt.
- **Reversible later**: the release pipeline has a Developer ID signing and notarization step that switches on once the secrets exist. The same DMG then becomes notarized, and the in-app plug-in install stays.
- **To test per DAW**: that app-copied, unnotarized plug-ins in the per-user folders load without a prompt.

Decided in [Choose how Das-Meter is packaged, installed and updated](https://github.com/daswerk/Das-Meter/issues/22). Research: [`docs/research/packaging-and-updates.md`](../research/packaging-and-updates.md).
