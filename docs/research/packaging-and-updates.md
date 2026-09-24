# Research: how to package, sign, install and update the app and the Send Plugin

Ticket: [How to package, sign, install and update the app and the Send Plugin](https://github.com/daswerk/Das-Meter/issues/17). Researched 2026-09-24. The decision it feeds is [Choose how Das-Meter is packaged, installed and updated](https://github.com/daswerk/Das-Meter/issues/22).

This file collects facts and leaves the choice open. Wording follows `CONTEXT.md` (**Send Plugin**, **System Capture**). The setting comes from the ADRs: a Rust app, one CLAP Send Plugin wrapped to VST3 and AUv2 by `clap-wrapper-rs` (ADR 0001), macOS 14.6+ (ADR 0002), Windows 10 22H2+, direct download only.

Each claim carries a source link. Where a link points into a Git repo, it is pinned to the commit I read. Claims that are my own reading, or that have no primary source I could open, are marked **(inference)** or **(unverified)**.

**Blocked sites.** The network proxy refused `learn.microsoft.com`, `azure.microsoft.com`, `sparkle-project.org`, `winsparkle.org`, `opensource.axo.dev`, `steinbergmedia.github.io`, `jrsoftware.org`, `docs.github.com` and `minimeters.app`. I read Microsoft, GitHub, Steinberg and axo docs from their GitHub source repos instead, and Apple docs through the `developer.apple.com/documentation/*.md` endpoints. Facts about MiniMeters come from search snippets only.

---

## 1. macOS

### 1.1 Developer ID signing, notarization and cost

- **Cost.** "The Apple Developer Program is 99 USD per membership year", in local currency where available. [Apple: Enroll](https://developer.apple.com/programs/enroll/), [What's included](https://developer.apple.com/programs/whats-included/)
  - To enrol as an organization you need a legal entity (no DBAs or trade names), a **D-U-N-S Number**, a work email on the organization's domain and a public website. [Apple: Enroll](https://developer.apple.com/programs/enroll/)
- **Certificates.** Only the Account Holder can create them (admins can too with the cloud-managed role). You can have up to five **Developer ID Application** certificates (to sign apps and code) and up to five **Developer ID Installer** certificates (to sign `.pkg` files). [Apple: Developer ID certificates](https://developer.apple.com/support/developer-id/)
- **Expiry.** Software signed while the certificate was valid keeps installing and running after the certificate expires. The same holds if the membership lapses, but you then can't get new certificates to sign updates. [Apple: Developer ID certificates](https://developer.apple.com/support/developer-id/)
- **Notarization is required.** "Beginning in macOS 10.15, all software built after June 1, 2019, and distributed with Developer ID must be notarized." Apple's notary service needs: [Apple: Notarizing macOS software](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
  - every executable signed with a Developer ID certificate;
  - the **Hardened Runtime** enabled for app and command-line targets;
  - a secure timestamp;
  - no `com.apple.security.get-task-allow` entitlement;
  - linking against the macOS 10.9 SDK or later.
- **Plug-ins must be notarized too.** "In macOS 10.15 and later, apps can load quarantined plug-ins … only if the plug-in is notarized." Otherwise the user must approve the plug-in in System Settings. [same page, "Notarize plug-ins"](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- **Plug-ins inherit the host's entitlements.** "Plug-ins don't declare their own entitlements. Instead, they inherit the entitlements of the host process." [same page](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) This means the Send Plugin runs with whatever entitlements the DAW has **(inference, consistent with ADR 0003)**.
- **Tools.** `notarytool` submits and `stapler` staples. The notary service stopped accepting `altool` on 1 November 2023. [Apple: Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
  - It accepts UDIF disk images, signed flat installer packages and ZIP archives. It processes nested containers and issues a ticket for each nested item, for example a `.pkg` inside a `.dmg`. [same](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
  - You can staple to an app, a bundle, a disk image or a flat package. You can't staple to a ZIP or to a bare binary. Without a stapled ticket, Gatekeeper looks the ticket up online. [same, "Staple the ticket"](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
  - Most submissions finish within 5 minutes and 98% within 15 minutes. Apple asks for at most 75 notarizations per day. [same](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
  - A **custom third-party installer** needs two rounds of notarization: first the payload, then the installer. [same](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow) Apple's own `.pkg`/Installer is not a "custom third-party installer" **(inference)**.
- **How to sign.** Sign from the inside out. Use `codesign -s "Developer ID Application" -f --timestamp`, and add `-o runtime` (Hardened Runtime) when signing a main executable. `codesign --deep` only signs nested code that sits in the places it expects. [Apple: Creating distribution-signed code for macOS](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac)
- **No Mac needed (alternative).** `apple-codesign` (`rcodesign`) is "a pure Rust (re)implementation of Apple code signing and notarization" that works "without macOS and without Apple hardware". It is licensed **MPL-2.0**, which is file-level copyleft; we would only run it as a tool, not link it. [apple-platform-rs README @0ebbd2e](https://github.com/indygreg/apple-platform-rs/blob/0ebbd2ef3b96d5adccc30b1cad4deea38cd735c4/README.md), [apple-codesign/Cargo.toml](https://github.com/indygreg/apple-platform-rs/blob/0ebbd2ef3b96d5adccc30b1cad4deea38cd735c4/apple-codesign/Cargo.toml)

### 1.2 DMG, ZIP or `.pkg`, and the plug-in folders

- **Apple's comparison of container formats.** [Apple: Packaging Mac software for distribution](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
  - **ZIP:** it can't be signed.
  - **DMG:** it can be signed. The user drags things out of it. It is "easiest if your product is a single file or bundle".
  - **`.pkg`:** it must be signed with **Developer ID Installer**. It is "the best choice if your product contains multiple components, must be copied to specific locations, or if you need to run custom code during installation".
  - Containers can be nested (for example a `.pkg` inside a `.dmg`). Sign each layer from the innermost out.
- **Tools.** `pkgbuild` builds component packages and `productbuild` builds the product (distribution) package, both signed with `--sign`. [same](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- **Plug-in folders on macOS.** Every format has a per-user folder and a system-wide folder, so a user-only install is possible for all three.

  | Format | Per-user | System-wide | Source |
  |---|---|---|---|
  | AU (`.component`) | `~/Library/Audio/Plug-Ins/Components` | `/Library/Audio/Plug-Ins/Components` | [AudioComponent.h](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/AudioToolbox.framework/Versions/A/Headers/AudioComponent.h) (scanned non-recursively) |
  | VST3 | `~/Library/Audio/Plug-ins/VST3/` (priority 1) | `/Library/Audio/Plug-ins/VST3/` | [VST3 dev portal: Plug-in Locations @3dcb7be](https://github.com/steinbergmedia/vst3_dev_portal/blob/3dcb7be336478075ecb8baf1513dd399a93acd31/src/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BLocations.md) |
  | CLAP | `~/Library/Audio/Plug-Ins/CLAP` | `/Library/Audio/Plug-Ins/CLAP` | [clap/entry.h](https://github.com/free-audio/clap/blob/main/include/clap/entry.h) |

- **Does putting plug-ins there need a `.pkg`?**
  - Nothing *requires* a `.pkg`. A DMG can hold the three plug-in bundles, and the user copies each into its folder by hand **(inference from Apple's DMG description)**.
  - A `.pkg` is Apple's recommended format when files "must be copied to specific locations". [Packaging Mac software](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
  - Writing to `/Library/...` needs admin rights, and the Installer asks for them **(inference)**.
  - A distribution package can offer an install into the user's home folder. The `domains` element's `enable_currentUserHome` allows it: "A home directory installation is done as the current user (not as root), and it cannot write outside of the home directory." If `domains` is missing, only the system domain is enabled. [Apple: Distribution XML reference](https://developer.apple.com/library/archive/documentation/DeveloperTools/Reference/DistributionDefinitionRef/Chapters/Distribution_XML_Ref.html)
- **Installer and signed-code updates.** Apple says its own update mechanisms, "the Installer app, and the `installer` command-line tool", aren't susceptible to the code-signing crash described in §3.4. [Apple: Updating Mac Software](https://developer.apple.com/documentation/security/updating-mac-software)

### 1.3 Entitlements and Info.plist keys

- **`NSAudioCaptureUsageDescription`** (app Info.plist): "A message that tells people why your app is requesting access to capture system audio on macOS." [Apple](https://developer.apple.com/documentation/bundleresources/information-property-list/nsaudiocaptureusagedescription)
  - Apple's tap sample says you "need to include the `NSAudioCaptureUsageDescription` key" to capture with a tap. The first recording from an aggregate device containing a tap triggers the "system audio recording" prompt. [Apple: Capturing system audio with Core Audio taps](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps)
- **`com.apple.security.device.audio-input`** is the Hardened Runtime entitlement that lets an app "record audio using the built-in microphone and access audio input using Core Audio". [Apple](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.device.audio-input)
  - **Open question (unverified):** whether a Hardened Runtime app needs this entitlement to read a process tap through an aggregate device. Apple's tap page names only the Info.plist key. Test it with a notarized build.
- **App Sandbox is not required** for Developer ID distribution. Apple's notarization requirements list the Hardened Runtime but not the sandbox. [Notarizing macOS software](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- **AU Info.plist.** An AU bundle's Info.plist has an `AudioComponents` array. Each entry has `type`, `subtype`, `manufacturer`, `name`, `version` and `factoryFunction`, plus either `sandboxSafe` or a `resourceUsage` dictionary. [AudioComponent.h](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/AudioToolbox.framework/Versions/A/Headers/AudioComponent.h)
  - The `resourceUsage` keys are `iokit.user-client`, `mach-lookup.global-name`, `network.client` and `temporary-exception.files.all.read-write`. The last one "will not be supported" "in a future OS release". [same](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/AudioToolbox.framework/Versions/A/Headers/AudioComponent.h)
- **What `clap-wrapper-rs` generates.** Its bundler writes an AU Info.plist whose `resourceUsage` is hard-coded to `network.client` and `temporary-exception.files.all.read-write`. [bundler/src/bundle.rs @95200bb](https://github.com/blepfx/clap-wrapper-rs/blob/95200bb4d3f1ea243660bb19d6401b279977b096/bundler/src/bundle.rs#L273-L278)
  - Adding `mach-lookup.global-name` (the fallback in ADR 0003) would mean patching or post-processing that plist **(inference)**.
  - The bundler signs only ad hoc (`codesign … -s -`). It does no Developer ID signing, no notarization and no universal merge. [bundler/src/util.rs](https://github.com/blepfx/clap-wrapper-rs/blob/95200bb4d3f1ea243660bb19d6401b279977b096/bundler/src/util.rs#L156-L174)
  - Its `--install` option copies to the **per-user** folders (`~/Library/Audio/Plug-Ins/…`, `%LOCALAPPDATA%\Programs\Common\…`). [util.rs](https://github.com/blepfx/clap-wrapper-rs/blob/95200bb4d3f1ea243660bb19d6401b279977b096/bundler/src/util.rs#L119-L150)
  - The crate is MIT OR Apache-2.0 and was "Tested on … MacOS (13.7) and Windows (10)". [README](https://github.com/blepfx/clap-wrapper-rs/blob/95200bb4d3f1ea243660bb19d6401b279977b096/README.md)

### 1.4 Universal binaries

- Apple: build each architecture, then "merge the resulting executable files into a single executable binary using the `lipo` tool" (`lipo -create -output universal x86 arm`). A universal binary can be built on either an Intel Mac or an Apple silicon Mac. [Apple: Building a universal macOS binary](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary)
- Rust: `aarch64-apple-darwin` is **Tier 1** (macOS 11.0+). `x86_64-apple-darwin` is now **Tier 2 with host tools** (macOS 10.12+). [rustc platform support](https://github.com/rust-lang/rust/blob/main/src/doc/rustc/src/platform-support.md)
- So a universal build means two `cargo build --target …` runs and `lipo` on the app binary and on each plug-in's inner binary, *before* signing **(inference)**.
- `cargo-dist` has no macOS universal bundling. It is an open request (issue #77). [cargo-dist book: installers @dbc3732](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/installers/index.md)

---

## 2. Windows

### 2.1 Code signing and SmartScreen

- **Microsoft's comparison** (Windows dev docs, dated 2026-04-20). [code-signing-options.md @aabd22a](https://github.com/MicrosoftDocs/windows-dev-docs/blob/aabd22a9113af71e56bd6f5af8fde06df68162e0/hub/apps/package-and-deploy/code-signing-options.md)

  | Option | Cost (per Microsoft) | Who can get it | SmartScreen |
  |---|---|---|---|
  | **Azure Artifact Signing** (formerly *Trusted Signing*) | "~$9.99/month" | Organizations: USA, Canada, EU, UK. Individuals: USA and Canada only | Reputation builds over time |
  | **OV certificate** (DigiCert, Sectigo, GlobalSign …) | "$150–300/year" | Worldwide | Same as Artifact Signing |
  | **EV certificate** | "$400+/year" | Worldwide | "Same as OV since 2024 — no longer instant bypass" |
  | Self-signed / unsigned | Free | — | "Windows protected your PC" block |

- **EV no longer buys reputation.** "EV certificates no longer bypass SmartScreen … Paying a premium for EV solely to avoid SmartScreen warnings is no longer justified." [smartscreen-reputation.md @aabd22a](https://github.com/MicrosoftDocs/windows-dev-docs/blob/aabd22a9113af71e56bd6f5af8fde06df68162e0/hub/apps/package-and-deploy/smartscreen-reputation.md)
- **How SmartScreen decides.** It checks **publisher reputation** and **file-hash reputation**. [same](https://github.com/MicrosoftDocs/windows-dev-docs/blob/aabd22a9113af71e56bd6f5af8fde06df68162e0/hub/apps/package-and-deploy/smartscreen-reputation.md)
  - New signed files can still warn "until … sufficient evidence of positive reputation". There is no exact threshold, but it "can take several weeks and hundreds of clean installs".
  - Signing every release lets the certificate's reputation carry over to new files. Unsigned files start from zero each version.
  - There is no way to submit a file for consumer SmartScreen review.
  - On Windows 11, **Smart App Control** "will block execution of unsigned files unless the file has a positive reputation".
- **Key storage.** Since June 2023, CA/B Forum rules require OV private keys on an HSM or hardware token (cloud HSM or USB). [code-signing-options.md](https://github.com/MicrosoftDocs/windows-dev-docs/blob/aabd22a9113af71e56bd6f5af8fde06df68162e0/hub/apps/package-and-deploy/code-signing-options.md)
- **Artifact Signing details** (Azure docs, [MicrosoftDocs/azure-docs @4c2e6ff](https://github.com/MicrosoftDocs/azure-docs/tree/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing)):
  - There are Basic and Premium SKUs. Billing is not pro-rated. The pricing page (`azure.microsoft.com`) was blocked here. [overview.md](https://github.com/MicrosoftDocs/azure-docs/blob/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing/overview.md), [faq.yml](https://github.com/MicrosoftDocs/azure-docs/blob/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing/faq.yml)
  - The certificates are "renewed daily and are valid for only 72 hours", so a timestamp countersignature is "critical". [concept-certificate-management.md](https://github.com/MicrosoftDocs/azure-docs/blob/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing/concept-certificate-management.md)
  - It issues no EV certificates, and there is "no plan to issue EV certificates in the future". Keys live in FIPS 140-3 Level 3 HSMs. [faq.yml](https://github.com/MicrosoftDocs/azure-docs/blob/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing/faq.yml)
  - **Eligibility differs between Microsoft's own pages.** The Azure quickstart lists organizations in the USA, Canada, EU, UK, Australia, New Zealand, Japan, South Korea, Singapore, Switzerland, Norway and Israel. Individuals must be in the USA or Canada. [quickstart.md](https://github.com/MicrosoftDocs/azure-docs/blob/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing/quickstart.md) The Windows dev-docs page lists fewer countries. Das Werk's country and legal form decide this.
  - Signing uses `SignTool.exe` (Windows SDK ≥ 10.0.2261.755) plus the `Microsoft.ArtifactSigning.Client` dlib and .NET 8, or the GitHub Action. [how-to-signing-integrations.md](https://github.com/MicrosoftDocs/azure-docs/blob/4c2e6ff188d706ffbbb93cfb87c55dd549b555c2/articles/artifact-signing/how-to-signing-integrations.md)
- **Free for open source.** "SignPath Foundation offers free code signing for qualifying open-source projects." [code-signing-options.md](https://github.com/MicrosoftDocs/windows-dev-docs/blob/aabd22a9113af71e56bd6f5af8fde06df68162e0/hub/apps/package-and-deploy/code-signing-options.md) I did not check its eligibility rules **(unverified)**.

### 2.2 Installer tools and their licences

| Tool | Licence | Notes |
|---|---|---|
| **WiX Toolset** | **MS-RL** (reciprocal) + **Open Source Maintenance Fee**: "if you use this project to generate revenue, the Maintenance Fee is required"; downloading releases requires adherence to the OSMF EULA. [LICENSE.TXT](https://github.com/wixtoolset/wix/blob/77aa9818ad37637f961afe143be88bdc38a3f350/LICENSE.TXT), [README](https://github.com/wixtoolset/wix/blob/77aa9818ad37637f961afe143be88bdc38a3f350/README.md) | Builds `.msi`. It is a build tool, not linked into our code. MS-RL's reciprocity would apply to WiX files shipped inside the MSI, such as WiX's own custom-action DLLs **(inference)**. |
| **Inno Setup** | Own permissive licence: "Permission is granted to anyone to use this software for any purpose, including commercial applications", keeping copyright notices. [license.txt](https://github.com/jrsoftware/issrc/blob/dd21bddcca863d260ddbdc7c7083175d2c3b47bf/license.txt) | The release notes say "Using Inno Setup commercially? Please purchase a license." [whatsnew.htm](https://github.com/jrsoftware/issrc/blob/dd21bddcca863d260ddbdc7c7083175d2c3b47bf/whatsnew.htm) Whether that is a request or a requirement is on `jrsoftware.org`, which was blocked **(unverified)**. Current version is 7.1. |
| **NSIS** | zlib/libpng. Compression modules are zlib, bzip2 and CPL-1.0 (LZMA), with a special exception that linking to LZMA doesn't subject your code to the CPL. [COPYING](https://github.com/kichik/nsis/blob/3ad3d52c55adffa58b844b00c9f608cc7a75640f/COPYING) | Builds `.exe` installers. |
| **cargo-dist** (`dist`) | MIT OR Apache-2.0. [Cargo.toml](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/Cargo.toml) | Installers: shell, PowerShell, npm, Homebrew, **MSI**. **No DMG, `.app` or `.pkg`** (DMG/app is issue #24). [installers/index.md](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/installers/index.md) The MSI needs **WiX v3** ("WiX v4 isn't yet supported"), through cargo-wix. [installers/msi.md](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/installers/msi.md) Windows signing through SSL.com eSigner or Azure Artifact Signing, x86_64 only. [signing/windows.md](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/supplychain-security/signing/windows.md) macOS: `codesign` only, "In the future, this module will also support notarization". [sign/macos.rs](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/cargo-dist/src/sign/macos.rs) It is built for CLI tools and has no concept of plug-in folders **(inference)**. |
| **cargo-packager** | Apache-2.0 OR MIT. [Cargo.toml](https://github.com/crabnebula-dev/cargo-packager/blob/9adb8f9b94c60c848e93a668c8658b50c961c2b8/Cargo.toml) | Outputs macOS `.app` and `.dmg`, Windows NSIS `.exe` and WiX `.msi`, Linux deb, AppImage and pacman. **No `.pkg`.** [README](https://github.com/crabnebula-dev/cargo-packager/blob/9adb8f9b94c60c848e93a668c8658b50c961c2b8/README.md) It notarizes with `notarytool` and reads `APPLE_CERTIFICATE`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_API_KEY` and more. On Windows it takes a certificate thumbprint, timestamp URL or custom `sign_command`. [config/mod.rs](https://github.com/crabnebula-dev/cargo-packager/blob/9adb8f9b94c60c848e93a668c8658b50c961c2b8/crates/packager/src/config/mod.rs) |

### 2.3 Plug-in folders and admin rights on Windows

| Format | Per-user | System-wide | Source |
|---|---|---|---|
| VST3 | `%LOCALAPPDATA%\Programs\Common\VST3\` (priority 1, "Mainly used for development use case") | `C:\Program Files\Common Files\VST3\` | [VST3 dev portal @3dcb7be](https://github.com/steinbergmedia/vst3_dev_portal/blob/3dcb7be336478075ecb8baf1513dd399a93acd31/src/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BLocations.md) |
| CLAP | `%LOCALAPPDATA%\Programs\Common\CLAP` | `%COMMONPROGRAMFILES%\CLAP` | [clap/entry.h](https://github.com/free-audio/clap/blob/main/include/clap/entry.h) |

- `%LOCALAPPDATA%\Programs\Common` is the known folder `FOLDERID_UserProgramFilesCommon` (per-user, since Windows 7). [knownfolderid.md @e103fa4](https://github.com/MicrosoftDocs/win32/blob/e103fa4e8810bd8d42c4777e17081e24dbe62dbd/desktop-src/shell/knownfolderid.md)
- **Admin rights.**
  - Writing under `C:\Program Files` needs elevation **(inference; standard UAC behaviour, no primary page fetched)**.
  - Inno Setup's default `PrivilegesRequired=admin` raises a UAC prompt. `lowest` runs in "non administrative install mode", where its `{autocf}` constant maps to the per-user Common Files folder instead of `{commoncf}`. [isetup.xml @dd21bdd](https://github.com/jrsoftware/issrc/blob/dd21bddcca863d260ddbdc7c7083175d2c3b47bf/ISHelp/isetup.xml)
- **Host support for the per-user folders** isn't guaranteed in every DAW, even though the specs list them **(unverified; worth a test matrix)**.

---

## 3. Updates

### 3.1 macOS: Sparkle

- **Sparkle 2**: MIT licence. [LICENSE @fd34238](https://github.com/sparkle-project/Sparkle/blob/fd34238cbbc5db4a6e8343c62ae4dea939b06ea4/LICENSE) Runtime is **macOS 12.0+** on `2.x`. [README](https://github.com/sparkle-project/Sparkle/blob/fd34238cbbc5db4a6e8343c62ae4dea939b06ea4/README.markdown)
  - Updates are "verified using EdDSA signatures and Apple Code Signing", with delta updates and RSS appcasts.
  - It "Supports applications, package installers, … and other plug-ins" and "Sparkle 2 supports updating external bundles".
  - It "Handles permissions, quarantine, and automatically asks for authentication if needed".
  - Tools: `generate_keys`, `sign_update`, `generate_appcast`, `sparkle-cli`.
- **From Rust.** Sparkle is an Objective-C framework. On crates.io, `sparkle-updater` 0.1.0 ("Main-thread Rust bindings for the macOS Sparkle update framework", MIT, 20 downloads) and `tauri-plugin-sparkle-updater` 0.3.0 (MIT) both come from one repo. [crates.io API](https://crates.io/api/v1/crates/sparkle-updater)
  - Both are very young. A hand-written `objc2` bridge is the other route **(inference)**.

### 3.2 Windows: WinSparkle

- **WinSparkle**: MIT licence. It has a C API and prebuilt `WinSparkle.dll` for x86, x64 and arm64, and uses the same appcast format as Sparkle. [README @0828b4f](https://github.com/vslavik/winsparkle/blob/0828b4fda11230907b65c48902ab6b3a2681a7ca/README.md), [COPYING](https://github.com/vslavik/winsparkle/blob/0828b4fda11230907b65c48902ab6b3a2681a7ca/COPYING)
  - Updates are signed with EdDSA (Ed25519); DSA is deprecated. The public key sits in a resource or is set through `win_sparkle_set_eddsa_public_key()`. `winsparkle-tool` generates keys and signs. [same](https://github.com/vslavik/winsparkle/blob/0828b4fda11230907b65c48902ab6b3a2681a7ca/README.md)
  - No `winsparkle` crate exists on crates.io, so we would call its C API through FFI **(inference)**.

### 3.3 Rust-native updaters

- **axoupdater** (MIT OR Apache-2.0). It works as a library or standalone program, reading releases from GitHub Releases. [README @73ba5a7](https://github.com/axodotdev/axoupdater/blob/73ba5a7eddf541b96e3db1c0b0243119a6115e15/README.md)
  - It depends on **cargo-dist install receipts** (`~/.config/APP` or `%LOCALAPPDATA%\APP`), so it only fits apps installed by dist's shell or PowerShell installers.
  - It uses unauthenticated GitHub API calls by default. dist marks the updater "experimental". [updater.md](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/installers/updater.md)
- **cargo-packager-updater.** It checks a JSON endpoint and verifies a `.sig` with minisign (`minisign-verify`) against an embedded public key. [crates/updater/README.md](https://github.com/crabnebula-dev/cargo-packager/blob/9adb8f9b94c60c848e93a668c8658b50c961c2b8/crates/updater/README.md)
- **self_update** (MIT, 1.3.0). Backends include GitHub, GitLab, S3 and a static manifest. [README @fc84045](https://github.com/jaemk/self_update/blob/fc840459d2ba11dec2b04a8db053e919730b1d13/README.md)
  - It can verify signatures with `zipsign` and checksums against GitHub's per-asset digest.
  - It has a "Bundle installs (macOS `.app`)" mode, because replacing only the inner executable "breaks the bundle's code signature".
- **Self-built "check for update".** The app fetches the newest GitHub Release and opens the download page or the installer. This needs no framework **(inference)**.
  - GitHub's unauthenticated API rate limit applies, as axoupdater notes. [updater.md](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/installers/updater.md)

### 3.4 Updating plug-ins while a DAW has them loaded

- **macOS.** "macOS caches information about the code's signature in the kernel. It doesn't flush that cache when you modify the file's contents", which can lead to a `Code Signature Invalid` crash. [Apple: Updating Mac Software](https://developer.apple.com/documentation/security/updating-mac-software)
  - The fix is to write to a temporary file and `rename()` it over the old one, never overwrite in place. This applies to "executables, frameworks, dynamic libraries, and bundles".
  - The Installer app and `installer` aren't affected.
  - A DAW that already has the old plug-in loaded keeps running the old code until it reloads **(inference)**.
- **Windows.** Microsoft's documented way to replace a DLL in use is to [Dynamic-Link Library Updates @e103fa4](https://github.com/MicrosoftDocs/win32/blob/e103fa4e8810bd8d42c4777e17081e24dbe62dbd/desktop-src/Dlls/dynamic-link-library-updates.md):
  1. rename the old DLL with `MoveFileEx` (same volume);
  2. copy the new one in;
  3. delete the renamed one with `MOVEFILE_DELAY_UNTIL_REBOOT`.

  Running processes keep the old DLL until they unload it. The doc warns this matters when a DLL "communicates with other services". That is exactly the Send Plugin ↔ app shared-memory table, which ADR 0003 versions by name (`dasmeter.v1`).
  - Inno Setup's `restartreplace` flag schedules in-use files for replacement at reboot, and only works when elevated. [isetup.xml](https://github.com/jrsoftware/issrc/blob/dd21bddcca863d260ddbdc7c7083175d2c3b47bf/ISHelp/isetup.xml)
  - The **Restart Manager** can close and restart applications holding files. Windows Installer 4.0+ uses it automatically. [About Restart Manager](https://github.com/MicrosoftDocs/win32/blob/e103fa4e8810bd8d42c4777e17081e24dbe62dbd/desktop-src/RstMgr/about-restart-manager.md) It would offer to close the DAW **(inference)**.
- **Each format is its own file.** clap-wrapper-rs builds the VST3 (and on macOS the AU) from the same CLAP code, but the installed `.clap`, `.vst3` and `.component` are separate files in separate folders. A DAW locks only the one it loaded, so each format has to be replaced on its own **(inference)**.

---

## 4. CI (GitHub Actions)

- **Cost.** GitHub-hosted standard runners are free for public repositories. Private repositories get a monthly minute quota by plan. [billing/github-actions.md @a9a4c3c](https://github.com/github/docs/blob/a9a4c3c0bbfcedf06f899b572140a5c8b9797309/content/billing/concepts/product-billing/github-actions.md) I didn't check the macOS minute multiplier for private repos **(unverified)**.
- **macOS signing on a runner.** GitHub's guide stores the `.p12` as Base64 (`BUILD_CERTIFICATE_BASE64`) with `P12_PASSWORD`, plus a random `KEYCHAIN_PASSWORD`. It creates a temporary keychain in `$RUNNER_TEMP`, imports the certificate and deletes the keychain afterwards. [sign-xcode-applications.md @a9a4c3c](https://github.com/github/docs/blob/a9a4c3c0bbfcedf06f899b572140a5c8b9797309/content/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications.md) cargo-dist's macOS signer does the same with an ephemeral keychain. [sign/macos.rs](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/cargo-dist/src/sign/macos.rs)
- **Notarization credentials.** There are two ways:
  - Apple ID + app-specific password + team ID. [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
  - An **App Store Connect API key**: the same key as the App Store Connect API, used to sign a JWT. [Apple: Submitting software for notarization over the web](https://developer.apple.com/documentation/notaryapi/submitting-software-for-notarization-over-the-web) That `notarytool` takes it as `--key`, `--key-id` and `--issuer` comes from its man page, which I didn't fetch **(unverified)**.
- **Artifact Signing on a runner.** Use `azure/artifact-signing-action@v2` (MIT), which runs **only on Windows runners** (windows-2022/2025; no Windows Arm). [README @349a715](https://github.com/Azure/artifact-signing-action/blob/349a71564a3106ed1344de68d989e4db07bb7dfb/README.md)
  - It recommends OIDC through `azure/login` with `AZURE_CLIENT_ID`, `AZURE_TENANT_ID` and `AZURE_SUBSCRIPTION_ID`. A client secret is the other option.
  - Inputs: `endpoint` (regional, e.g. `https://weu.codesigning.azure.net/`), `signing-account-name`, `certificate-profile-name`, and the files to sign.
- **Summary of secrets for both OSes (inference, assembled from the sources above):**

  | Purpose | Secrets |
  |---|---|
  | macOS code signing | Developer ID Application `.p12` (Base64) + password; keychain password |
  | macOS `.pkg` (only if we ship one) | Developer ID Installer `.p12` (Base64) + password |
  | Notarization | App Store Connect API key (`.p8`, key ID, issuer ID), *or* Apple ID + app-specific password + team ID |
  | Windows, Artifact Signing | Azure tenant, client and subscription IDs (OIDC, so no long-lived secret), or a client secret |
  | Windows, OV certificate from a cloud-HSM vendor | Vendor credentials, e.g. SSL.com eSigner: username, password, TOTP secret, credential ID ([dist docs](https://github.com/axodotdev/cargo-dist/blob/dbc373255abc9581f95ee51d1bc8b59bc8002169/book/src/supplychain-security/signing/windows.md)) |
  | Update signing (Sparkle / WinSparkle / minisign / zipsign) | The EdDSA or minisign private key |

- **Cross-builds.** A universal macOS build can run on one macOS runner. `lipo` works on either architecture. [Apple](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary) `rcodesign` could sign and notarize from Linux instead. [apple-platform-rs](https://github.com/indygreg/apple-platform-rs/blob/0ebbd2ef3b96d5adccc30b1cad4deea38cd735c4/README.md)

---

## 5. How similar tools ship (facts only; no GPL code read for reuse)

- **MiniMeters (unverified: search snippets of `minimeters.app`, which was blocked).**
  - macOS ships as `MiniMeters.pkg`, which by default installs the app plus the CLAP, AU and VST3 MiniMetersServer. [Installation](https://minimeters.app/help/installation/)
  - Plug-in locations: `/Library/Audio/Plug-Ins/{Components,VST3,CLAP}` on macOS, and `C:\Program Files\Common Files\{VST3,CLAP}` on Windows, where the server is "only included as a VST3". [MiniMetersServer Setup](https://minimeters.app/help/minimetersserver-setup/)
  - It is also sold on itch.io. [itch.io](https://directmusic.itch.io/minimeters)
- **Surge XT** (JUCE, GPL-3; I read only its build scripts for facts). [make_installer.sh @fd50e7b](https://github.com/surge-synthesizer/surge/blob/fd50e7b3df13f601e50093f8efa73bcfbcc8c22c/scripts/installer_mac/make_installer.sh), [surge64.iss](https://github.com/surge-synthesizer/surge/blob/fd50e7b3df13f601e50093f8efa73bcfbcc8c22c/scripts/installer_win/surge64.iss), [entitlements.plist](https://github.com/surge-synthesizer/surge/blob/fd50e7b3df13f601e50093f8efa73bcfbcc8c22c/scripts/installer_mac/Resources/entitlements.plist)
  - **macOS:**
    - Each format gets its own signed `pkgbuild` component, installed to `/Library/Audio/Plug-Ins/{VST3,Components,CLAP,LV2}`. They are combined with `productbuild` using `<domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>`, so the install is system-wide only.
    - The `.pkg` goes into a signed DMG, which is notarized with `notarytool … --wait` and stapled.
    - The standalone app is signed with `-o runtime`. Its entitlements are `allow-jit`, `allow-unsigned-executable-memory`, `device.audio-input` and `disable-library-validation`.
  - **Windows:** Inno Setup with `{commoncf64}\VST3\…` and `{commoncf64}\CLAP\…` destinations, so it needs admin rights.
- **Pattern (inference):** both audio tools install plug-ins system-wide through an OS-native installer (`.pkg` on macOS, an elevated installer on Windows). The Rust packaging tools (cargo-dist, cargo-packager) don't handle plug-in folders.

---

## 6. Open questions to test, not researched further

1. Does a Hardened Runtime, notarized app need `com.apple.security.device.audio-input` to read a Core Audio process tap? (§1.3)
2. Do the major DAWs scan the **per-user** VST3 and CLAP folders on Windows and macOS? (§2.3)
3. What are the Inno Setup commercial-licence terms (`jrsoftware.org`, blocked), and do the WiX OSMF terms apply if Das-Meter is free? (§2.2)
4. Is Das Werk eligible for Azure Artifact Signing (country, organization vs individual)? What does the Basic SKU cost now (pricing page blocked)? (§2.1)
5. Does Logic's `AUHostingService` accept the `resourceUsage` that clap-wrapper-rs generates? This ties into [Test the Send Plugin's shared memory inside Logic Pro](https://github.com/daswerk/Das-Meter/issues/16).
