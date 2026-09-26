# Research: Linux packaging conventions (AUR, AppImage, desktop integration)

Ticket: [Research: Linux packaging conventions (AUR, AppImage, desktop integration)](https://github.com/daswerk/Das-Meter/issues/81), part of the map [Linux, done properly](https://github.com/daswerk/Das-Meter/issues/76). Researched 2026-09-26. The decision it feeds is [Packaging, updates and desktop integration on Linux](https://github.com/daswerk/Das-Meter/issues/86).

This file collects facts and leaves the choices open. Wording follows `CONTEXT.md` (**Send Plugin**, **System Capture**, **Preset**). The setting comes from the map (#76): AUR package plus AppImage (no Flatpak, `.deb` or `.rpm`); the AUR package puts the Send Plugin into `/usr/lib/{clap,vst3}`; the AppImage's **Install Send Plugin…** copies it into `~/.clap` and `~/.vst3`; AppImage updates are a daily check plus a note. Must-work target: Arch, Wayland, GNOME and KDE Plasma, x86_64.

Each claim carries a source link. Links into Git repos are pinned to the commit I read. Claims that are my own reading are marked **(inference)**; claims with no primary source I could open are marked **(unverified)**.

**Access notes.** `wiki.archlinux.org` answers normal fetches with an Anubis bot wall. I read the Arch wiki pages as wikitext (`index.php?title=…&action=raw`), so the content is the live page on 2026-09-26. The Rust package guidelines have moved from the wiki to the Arch **Developer Manual** (the wiki page is now a redirect). Arch package facts (dependencies, provides, file lists) come from the `archlinux.org/packages/…/json/` API.

---

## 0. The short answer

- **AUR.** Both a source package (`das-meter`) and a prebuilt one (`das-meter-bin`) are allowed. A package that repackages our release tarball **must** carry the `-bin` suffix. Publishing is a `git push` over SSH to `aur.archlinux.org` from an account with your public key. A GitHub Action can do it (`KSXGitHub/github-actions-deploy-aur`), but the AUR warns that automated updates are "at your own risk".
- **The app links few system libraries.** It links only glibc, libgcc, `libpipewire-0.3` and `libasound`. winit and wgpu `dlopen` Wayland, X11, xkbcommon, Vulkan and EGL at run time **(inference from crate features, §1.4)**. On the AppImage excludelist, glibc, `libpipewire-0.3.so.0`, `libasound.so.2`, `libEGL`/`libGL`, `libwayland-client` and `libX11` all appear. **So the AppImage needs to bundle no shared libraries at all.** A hand-made AppDir (binary, `.desktop`, icons, `AppRun`) run through `appimagetool` is enough **(inference)**.
- **glibc baseline.** Build on ubuntu-24.04 (glibc 2.39). The `pipewire` crate feature we use needs libpipewire ≥ 0.3.53, and Ubuntu 22.04 only has 0.3.48 **(inference)**. Arch is on glibc 2.44, so this is no problem for the must-work target.
- **Desktop integration uses the same files for both formats.** You need:
  - one `.desktop` file, named after a reverse-DNS app ID that also matches winit's Wayland `app_id`;
  - hicolor icons (48×48 at minimum, plus `scalable/apps/*.svg`);
  - a shared-mime-info XML file for `.dasmeter-preset`.

  The AUR package installs these under `/usr/share`, and pacman hooks refresh the caches. The AppImage carries them inside. If it copies them into `~/.local/share` on first run, the AppImage spec says it **SHOULD ask first**, and it should stand down when `appimaged` or a `no_desktopintegration` flag is present.
- **Launch at Login** means writing a `.desktop` file into `~/.config/autostart/` (XDG Autostart spec). For the AppImage, its `Exec=` must point at `$APPIMAGE`, the real file path, not at the temporary mount.

---

## 1. AUR

### 1.1 `-bin` versus a source package

- **Naming rules.** "Packages that use **prebuilt** deliverables, when the sources are available, must use the `-bin` suffix." "Packages that build from source using a specific version do not use a suffix." Packages that build from VCS HEAD use `-git`. [AUR submission guidelines, "Rules of submission"](https://wiki.archlinux.org/title/AUR_submission_guidelines#Rules_of_submission)
- So the options are `das-meter` (builds a tagged release from source), `das-meter-bin` (repackages the GitHub release tarball) and `das-meter-git` (builds the latest commit). They can coexist; the usual pattern is `provides=(das-meter)` and `conflicts=(das-meter)` on the `-bin`/`-git` variants. The guidelines say to use `conflicts` (and `provides` if other packages need it) for "an alternate version of an already existing package", and not `replaces`. [same](https://wiki.archlinux.org/title/AUR_submission_guidelines#Rules_of_submission)
- **Name availability.** On 2026-09-26 the AUR RPC returns no package named or matching `das-meter` or `dasmeter`, and no official Arch package matches `das-meter`. [AUR RPC search](https://aur.archlinux.org/rpc/v5/search/das-meter?by=name), [Arch package search](https://archlinux.org/packages/?q=das-meter)
- **x86_64 is required.** "Packages that do not support the `x86_64` architecture are not allowed in the AUR." [AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines#Rules_of_submission)
- **Trade-offs** **(inference)**:

  | | `das-meter-bin` | `das-meter` (source) |
  |---|---|---|
  | User's build time | seconds (download + copy) | a full `cargo build --release` of the workspace (egui, wgpu, …) on the user's machine |
  | makedepends | none | `cargo` (from `rust`), `clang` (for `pipewire-sys` bindgen, §1.3) |
  | Needs from our release | a tarball containing everything the package installs (app, Send Plugin `.clap`/`.vst3`, `.desktop`, icons, MIME XML, licences) | only the tag's source archive; the repo must be able to build the Send Plugin bundles on Linux |
  | Build environment | ubuntu-24.04 (glibc 2.39), runs on Arch's newer glibc | user's own Arch, so matches its libraries exactly |
  | Update work per release | new `pkgver` and checksums | same |
  | Rust MSRV | none on the user side | the app needs Rust 1.95 (`crates/app/Cargo.toml`); Arch's `rust` is 1.98.1 today ([Arch package](https://archlinux.org/packages/extra/x86_64/rust/)) |

### 1.2 PKGBUILD conventions that matter here

- **Licence.** The field "lists the packaged software's upstream license … The licenses in this field must be in the SPDX license format." [Arch package guidelines, "Licenses"](https://wiki.archlinux.org/title/Arch_package_guidelines#Licenses) Combined licences "should follow the SPDX syntax", e.g. `'GPL-2.0-or-later OR LGPL-2.1-or-later'`. [PKGBUILD, "license"](https://wiki.archlinux.org/title/PKGBUILD#license) So: `license=('MIT OR Apache-2.0')`. That is one array element holding the SPDX expression, matching our `Cargo.toml` **(inference from the example)**.
  - MIT needs its text installed: "License families like BSD or MIT are, strictly speaking, not a single license and each instance requires a separate license file … provide the corresponding file as if it was a custom license", in `/usr/share/licenses/$pkgname/`. [PKGBUILD, "license"](https://wiki.archlinux.org/title/PKGBUILD#license) Apache-2.0 is in the common `licenses` package (`/usr/share/licenses/spdx/`), but installing both files is harmless **(inference)**:
    `install -Dm644 LICENSE-MIT LICENSE-APACHE -t "$pkgdir/usr/share/licenses/$pkgname/"`.
  - Separate from that, the AUR asks for a `LICENSE` file (0BSD recommended) for the **PKGBUILD repo itself**. Packages without it "are not eligible for promotion to the official repositories". [AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines#Rules_of_submission), [Arch package guidelines, "Package sources licenses"](https://wiki.archlinux.org/title/Arch_package_guidelines#Package_sources_licenses)
- **depends.** "The `depends` array should list all direct first level dependencies even when some are already declared transitively." [PKGBUILD, "depends"](https://wiki.archlinux.org/title/PKGBUILD#depends) The Rust guidelines add that "most Rust binaries do link against glibc libraries, so `libgcc` and `glibc` are typically dependencies". [Arch Developer Manual: Rust](https://manual.archlinux.page/package-guidelines/rust/) Optional features go into `optdepends` with a reason (`'pkg: why'`). [Arch package guidelines](https://wiki.archlinux.org/title/Arch_package_guidelines#Package_etiquette)
- **Proposed depends for Das-Meter** **(inference from §1.4; namcap on a real build is the check)**:

  | Package | Why | Soname it provides |
  |---|---|---|
  | `glibc`, `libgcc` | every Rust binary | `libgcc_s.so=1-64` |
  | `libpipewire` | System Capture (cpal `pipewire` feature, linked) | `libpipewire-0.3.so=0-64` |
  | `alsa-lib` | cpal's ALSA backend, linked (`alsa-sys`) | `libasound.so=2-64` |
  | `pipewire` | the running PipeWire daemon, not just the client library (a user without it gets no System Capture) | — |
  | `vulkan-icd-loader` | wgpu's Vulkan backend, dlopened | `libvulkan.so=1-64` |
  | `libglvnd` | wgpu's GL/EGL fallback, and the Send Plugin's OpenGL window, dlopened | `libEGL.so`, `libGL.so` |
  | `wayland` | winit on Wayland, dlopened | `libwayland-client.so=0-64` |
  | `libxkbcommon` | keyboard handling, dlopened (`xkbcommon-dl`) | `libxkbcommon.so=0-64` |
  | `libx11`, `libxcursor`, `libxrandr`, `libxi`, `libxcb` | winit's X11 backend (XWayland Bar) and the Send Plugin window (baseview) | — |
  | `hicolor-icon-theme` | owns the `/usr/share/icons/hicolor` tree we install into (the Arch `surge-xt` package does the same) | — |
  | optdepends `vulkan-driver` | a Vulkan driver (`vulkan-intel`, `vulkan-radeon`, `nvidia-utils`, …) | — |
  | optdepends `xdg-desktop-portal` | file dialogs for Preset import and export (`rfd`'s `xdg-portal` feature) | — |

  Package facts: [libpipewire](https://archlinux.org/packages/extra/x86_64/libpipewire/), [alsa-lib](https://archlinux.org/packages/extra/x86_64/alsa-lib/), [vulkan-icd-loader](https://archlinux.org/packages/extra/x86_64/vulkan-icd-loader/), [libglvnd](https://archlinux.org/packages/extra/x86_64/libglvnd/), [wayland](https://archlinux.org/packages/extra/x86_64/wayland/), [libxkbcommon](https://archlinux.org/packages/extra/x86_64/libxkbcommon/), [libgcc](https://archlinux.org/packages/core/x86_64/libgcc/).
- **Plug-ins in `/usr/lib/clap` and `/usr/lib/vst3`.**
  - CLAP's search path on Linux is `~/.clap` and `/usr/lib/clap`, plus any `CLAP_PATH` directories, searched recursively for `*.clap`. [clap/entry.h @a47f6ba](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/entry.h)
  - VST3 on Linux looks in `$HOME/.vst3/`, `/usr/lib/vst3/`, `/usr/local/lib/vst3/` and `$APPFOLDER/vst3/`. [VST3 dev portal: Plug-in Locations @3dcb7be](https://github.com/steinbergmedia/vst3_dev_portal/blob/3dcb7be336478075ecb8baf1513dd399a93acd31/src/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BLocations.md) Our package can't use `/usr/local`: "Packages should **never** be installed to `/usr/local/`." [Arch package guidelines](https://wiki.archlinux.org/title/Arch_package_guidelines#Package_etiquette)
  - A Linux VST3 is a bundle: `Name.vst3/Contents/x86_64-linux/Name.so`. [VST3 dev portal: Plugin Format @3dcb7be](https://github.com/steinbergmedia/vst3_dev_portal/blob/3dcb7be336478075ecb8baf1513dd399a93acd31/src/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BFormat.md)
  - **Precedent in Arch.** `surge-xt` is split into `surge-xt-clap` (file `usr/lib/clap/Surge XT.clap`, group `clap-plugins`, depends on the virtual `clap-host`), `surge-xt-vst3` (group `vst3-plugins`, depends on `vst3-host`) and `surge-xt-standalone`. [surge-xt-clap](https://archlinux.org/packages/extra/x86_64/surge-xt-clap/), [surge-xt-vst3](https://archlinux.org/packages/extra/x86_64/surge-xt-vst3/)
    - `clap-host` and `vst3-host` are provided by hosts such as `reaper` and `qtractor`, and by the AUR's `bitwig-studio`. [reaper](https://archlinux.org/packages/extra/x86_64/reaper/), [AUR RPC: bitwig-studio](https://aur.archlinux.org/rpc/v5/info?arg[]=bitwig-studio)
    - For us, a single package with the plug-ins listed as `optdepends=('clap-host: …' 'vst3-host: …')`, or a split package, would both follow precedent. Adding `groups=(pro-audio clap-plugins vst3-plugins)` makes it show up with other audio plug-ins **(inference)**.
- **Sources and checksums.**
  - Sources must be unique in `SRCDEST`, so rename them if needed: `"${pkgname}-${pkgver}.tar.gz::https://…"`. [Arch package guidelines, "Package sources"](https://wiki.archlinux.org/title/Arch_package_guidelines#Package_sources)
  - A binary tarball for one architecture goes in `source_x86_64=()`, with a matching `sha256sums_x86_64=()`. [PKGBUILD, "source"](https://wiki.archlinux.org/title/PKGBUILD#source)
  - "The checksum type and values should always be those provided by upstream … the strongest checksum is to be preferred": b2, then sha512, then sha256. `updpkgsums` (from `pacman-contrib`) rewrites them in place. [PKGBUILD, "Integrity"](https://wiki.archlinux.org/title/PKGBUILD#Integrity)
  - GitHub already shows a SHA-256 `digest` for every release asset. For `v0.1.0-test3`, for example: `das-meter-0.1.0-linux-x86_64.tar.gz` → `sha256:0ffd43ca…`. (Source: `gh api repos/daswerk/Das-Meter/releases`.) Publishing a `SHA256SUMS` (or b2) file with the release is what "provided by upstream" would point at **(inference)**.
  - If signatures are published, add them to `source` and put the key fingerprint in `validpgpkeys`. [PKGBUILD, "Integrity"](https://wiki.archlinux.org/title/PKGBUILD#Integrity) minisign signatures, which we already use on macOS, are not something makepkg verifies **(inference)**.
- **Versions.** `pkgver` "can contain letters, numbers, periods and underscores, but **not** a hyphen". [PKGBUILD, "pkgver"](https://wiki.archlinux.org/title/PKGBUILD#pkgver) So a test tag such as `v0.1.0-test3` would have to become `0.1.0_test3`. More simply, the AUR package tracks only real releases **(inference)**.
- **Other rules.**
  - Use `/usr/lib/$pkgname/`, never `/usr/libexec`. [Arch package guidelines](https://wiki.archlinux.org/title/Arch_package_guidelines#Package_etiquette)
  - Put a `# Maintainer:` comment at the top of the PKGBUILD. [AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines#Rules_of_submission)

### 1.3 Source-package template (Arch Rust guidelines)

From the [Arch Developer Manual: Rust](https://manual.archlinux.page/package-guidelines/rust/):

- `makedepends=(cargo)`. The `rust` package provides `cargo`. [rust](https://archlinux.org/packages/extra/x86_64/rust/)
- `prepare()`: `export RUSTUP_TOOLCHAIN=stable; cargo fetch --locked --target host-tuple`. This lets the build run offline.
- `build()`: `export RUSTUP_TOOLCHAIN=stable; export CARGO_TARGET_DIR=target; cargo build --frozen --release --all-features`. The manual says `--features …` may replace `--all-features`. We would build `-p dasmeter-app` and `-p dasmeter-send` **(inference)**.
- `check()`: `cargo test --frozen --all-features`, plus `--workspace` "if the repository is a cargo workspace". Don't use `--release` for tests.
- `package()`: `install -Dm0755 -t "$pkgdir/usr/bin/" "target/release/$pkgname"`. Our binary is `das-meter`, which matches the package name.
- **GCC LTO note.** The manual warns about LTO link errors with mixed Rust/C projects and suggests `options=(!lto)`. Whether that affects us would show up on the first build **(unverified for us)**.
- **Extra makedepends for us.** `pipewire-sys` generates bindings with `bindgen` (`runtime` feature, so libclang at build time) and finds PipeWire through `system-deps`/pkg-config. [pipewire-sys Cargo.toml](https://gitlab.freedesktop.org/pipewire/pipewire-rs/-/blob/main/pipewire-sys/Cargo.toml) So: `makedepends=(cargo clang)`. `pkgconf` is in `base-devel` **(inference)**. Our CI already installs `libclang-dev` for the same reason (`.github/workflows/release.yml`).

### 1.4 What the binary really links (basis for depends and AppImage bundling)

These are **inferences from crate manifests**. The check is `readelf -d das-meter | grep NEEDED` on the release build, or `namcap` on the built package.

- **Linked (in `DT_NEEDED`):**
  - `libpipewire-0.3.so.0`: cpal 0.18's `pipewire` feature pulls in `pipewire` 0.10 with feature `v0_3_53`. [cpal Cargo.toml @v0.18.2](https://github.com/RustAudio/cpal/blob/v0.18.2/Cargo.toml)
  - `libasound.so.2`: cpal's ALSA backend (`alsa` 0.11). [same](https://github.com/RustAudio/cpal/blob/v0.18.2/Cargo.toml)
  - glibc, libm and `libgcc_s`.
- **dlopened (not in `DT_NEEDED`):**
  - Wayland: our app depends on `winit = "0.30.13"` with default features. Those include `wayland-dlopen`, which sets `wayland-backend/dlopen`. [winit Cargo.toml @v0.30.13](https://github.com/rust-windowing/winit/blob/v0.30.13/Cargo.toml)
  - X11: winit's `x11` feature uses `x11-dl` and `xkbcommon-dl`, both runtime loaders. [same](https://github.com/rust-windowing/winit/blob/v0.30.13/Cargo.toml)
  - Vulkan: wgpu-hal uses `ash`, whose default feature is `loaded` (not `linked`). [ash Cargo.toml @0.38.0](https://github.com/ash-rs/ash/blob/0.38.0/ash/Cargo.toml), [wgpu-hal Cargo.toml @v30.0.0](https://github.com/gfx-rs/wgpu/blob/v30.0.0/wgpu-hal/Cargo.toml)
  - EGL/GL: `khronos-egl` with feature `dynamic`. [wgpu-hal Cargo.toml](https://github.com/gfx-rs/wgpu/blob/v30.0.0/wgpu-hal/Cargo.toml)
  - File dialogs: `rfd` with `xdg-portal` talks D-Bus to the desktop portal.
- **The Send Plugin** (a `cdylib`) draws with `baseview` + `egui-baseview` (OpenGL). baseview's Linux build needs `libx11-dev libxcb1-dev libx11-xcb-dev libgl1-mesa-dev`. [baseview README @237d323](https://github.com/RustAudio/baseview/blob/237d323c729f3aa99476ba3efa50129c5e86cad3/README.md) So it is X11 only, and in a Wayland DAW it runs through XWayland **(inference)**.

### 1.5 Publishing and updating on the AUR

- **Account and key.**
  - "For write access to the AUR, you need to have an SSH key pair. The content of the public key needs to be copied to your profile in *My Account*." Configure it for `Host aur.archlinux.org` with `User aur`.
  - "You should create a new key pair rather than use an existing one, so that you can selectively revoke the keys." A profile can hold several public keys, one per line, so a dedicated CI key can sit next to the owner's own key.

  [AUR submission guidelines, "Authentication"](https://wiki.archlinux.org/title/AUR_submission_guidelines#Authentication)
- **The repo.**
  - Each package is a Git repo at `ssh://aur@aur.archlinux.org/<pkgbase>.git`. Cloning a name that doesn't exist yet gives an empty repo; the first push creates the package.
  - "The AUR only allows pushes to the `master` branch."
  - Every commit must contain `PKGBUILD` and `.SRCINFO` (`makepkg --printsrcinfo > .SRCINFO`), or the push is refused.
  - Commits carry the global Git name and e-mail, which are "very difficult to change" after pushing, so set `git config user.name/user.email` per repo.

  [AUR submission guidelines, "Creating package repositories" and "Publishing new package content"](https://wiki.archlinux.org/title/AUR_submission_guidelines#Publishing_new_package_content)
- **Updating.** Bump `pkgver` (reset `pkgrel=1`) for a new release. Bump only `pkgrel` for packaging fixes. Don't bump either for typo fixes. Always regenerate `.SRCINFO`. [same](https://wiki.archlinux.org/title/AUR_submission_guidelines#Publishing_new_package_content)
- **Automation is allowed, with a caveat.** "Automation is a valuable tool for maintainers, but it can not replace manual intervention … Automated `PKGBUILD` updates are used at your own risk and any malfunctioning accounts and their packages may be removed without prior notice." [AUR submission guidelines, "Maintaining packages"](https://wiki.archlinux.org/title/AUR_submission_guidelines#Maintaining_packages)
- **Existing GitHub Action.** [`KSXGitHub/github-actions-deploy-aur` @9901fcc](https://github.com/KSXGitHub/github-actions-deploy-aur/blob/9901fcce90f4089145583a9ac8e828b0049e3a27/README.md) is active (last push 2026-07-20).
  - It runs in an Arch Docker container.
  - Inputs: `pkgname`, `pkgbuild`, `commit_username`, `commit_email`, `ssh_private_key`, optionally `updpkgsums: true` and `test: true` (runs `makepkg --clean --cleanbuild --nodeps`).
  - It generates `.SRCINFO` itself ([build.sh](https://github.com/KSXGitHub/github-actions-deploy-aur/blob/9901fcce90f4089145583a9ac8e828b0049e3a27/build.sh)).
  - The private key would live as a secret in the `release` environment, like the macOS signing secrets **(inference)**.
- **Tracking upstream.** Arch suggests `nvchecker` or `urlwatch` to hear about new releases. [Arch package guidelines, "Working with upstream"](https://wiki.archlinux.org/title/Arch_package_guidelines#Working_with_upstream) Users flag packages out of date through the AUR web page. [Arch User Repository](https://wiki.archlinux.org/title/Arch_User_Repository#Flagging_packages_out-of-date)
- **Uninstall.** `pacman -R` removes every file the package installed (both plug-ins, the `.desktop` file, icons, MIME XML). Nothing the app wrote under `~/.config` or `~/.local` is touched, including an autostart entry it created **(inference)**.

### 1.6 Sketch: `das-meter-bin` PKGBUILD

An illustration assembled from the rules above, not tested. The tarball layout it assumes (`usr/…` tree inside) is a proposal; today's tarball holds only `das-meter`, licences and a README.

```bash
# Maintainer: … <… at … dot …>
pkgname=das-meter-bin
_pkgname=das-meter
pkgver=0.1.0
pkgrel=1
pkgdesc='Audio meters for the desktop and a Send Plugin for your DAW'
arch=('x86_64')
url='https://github.com/daswerk/Das-Meter'
license=('MIT OR Apache-2.0')
depends=('glibc' 'libgcc' 'libpipewire' 'pipewire' 'alsa-lib' 'vulkan-icd-loader' 'libglvnd'
         'wayland' 'libxkbcommon' 'libx11' 'libxcursor' 'libxrandr' 'libxi' 'libxcb' 'hicolor-icon-theme')
optdepends=('vulkan-driver: GPU rendering'
            'xdg-desktop-portal: file dialogs for Preset import and export'
            'clap-host: to use the CLAP Send Plugin'
            'vst3-host: to use the VST3 Send Plugin')
provides=("$_pkgname")
conflicts=("$_pkgname")
groups=('pro-audio' 'clap-plugins' 'vst3-plugins')
options=('!strip' '!debug')   # already stripped upstream (inference)
source_x86_64=("$_pkgname-$pkgver-x86_64.tar.gz::$url/releases/download/v$pkgver/$_pkgname-$pkgver-linux-x86_64.tar.gz")
sha256sums_x86_64=('…')

package() {
  cd "$_pkgname-$pkgver-linux-x86_64"
  install -Dm755 das-meter -t "$pkgdir/usr/bin/"
  install -d "$pkgdir/usr/lib/clap" "$pkgdir/usr/lib/vst3"
  cp -r "Das-Meter Send.clap" "$pkgdir/usr/lib/clap/"
  cp -r "Das-Meter Send.vst3" "$pkgdir/usr/lib/vst3/"
  cp -r share "$pkgdir/usr/"                 # applications/, icons/hicolor/, mime/packages/
  install -Dm644 LICENSE-MIT LICENSE-APACHE -t "$pkgdir/usr/share/licenses/$pkgname/"
}
```

Arch's pacman hooks already run `update-desktop-database`, `update-mime-database` and `gtk-update-icon-cache` when files land in those directories. The hooks ship in [desktop-file-utils](https://archlinux.org/packages/extra/x86_64/desktop-file-utils/), [shared-mime-info](https://archlinux.org/packages/extra/x86_64/shared-mime-info/) and [gtk-update-icon-cache](https://archlinux.org/packages/extra/x86_64/gtk-update-icon-cache/). So no `.install` script is needed **(inference from the hook file lists)**.

---

## 2. AppImage

### 2.1 What an AppImage is, and the tools

- **Format.** An AppImage is an ELF runtime with a SquashFS image of an **AppDir** appended. When run, it mounts the image and executes `AppRun`. [AppImageSpec draft @5220f3e, "Type 2 image format"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#type-2-image-format)
  - The recommended file name is `ApplicationName-$VERSION-$ARCH.AppImage`.
  - It "SHOULD NOT be encapsulated in another archive" (so no `.tar.gz` around it).
- **The AppDir must contain** [AppDir specification](https://docs.appimage.org/reference/appdir.html), [AppImageSpec, "Contents of the image"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#contents-of-the-image):
  - `AppRun`, which can be a symlink to the binary;
  - exactly one `*.desktop` file in the root;
  - an icon in the root named after `Icon=`;
  - a `.DirIcon` (256×256 PNG recommended);
  - by convention, a `usr/` tree with `bin/`, `share/applications/` and `share/icons/hicolor/…`.
- **The runtime.**
  - The current runtime is static (musl): "libfuse2 is no longer required on the target system." [type2-runtime README @75849dc](https://github.com/AppImage/type2-runtime/blob/75849dce7cc37e4319b633df1f116ca895c71a12/README.md)
  - It still needs a setuid `fusermount*` binary on `PATH` to mount. On Arch that is `fusermount3` from `fuse3` **(inference)**. [runtime.c](https://github.com/AppImage/type2-runtime/blob/75849dce7cc37e4319b633df1f116ca895c71a12/src/runtime/runtime.c)
  - Without FUSE, `--appimage-extract-and-run` or `APPIMAGE_EXTRACT_AND_RUN=1` extracts the image to a temporary directory and runs it from there. This is also the usual trick for running tool AppImages inside CI containers. [runtime.c](https://github.com/AppImage/type2-runtime/blob/75849dce7cc37e4319b633df1f116ca895c71a12/src/runtime/runtime.c)
  - The runtime sets **`$APPIMAGE`** (absolute path of the `.AppImage` file), **`$APPDIR`** (mount point) and `$ARGV0` for the payload. [runtime.c](https://github.com/AppImage/type2-runtime/blob/75849dce7cc37e4319b633df1f116ca895c71a12/src/runtime/runtime.c) The app needs `$APPIMAGE` for anything it writes outside (desktop entry, autostart), because `$APPDIR` changes on every run.
- **Tools** (all active in 2026):

  | Tool | What it does | Notes |
  |---|---|---|
  | [`appimagetool`](https://github.com/AppImage/appimagetool/blob/8c8c91f762b412a19f4e8d2c4b35afb98f2d7c81/README.md) (AppImage/appimagetool) | AppDir → AppImage. `-u` embeds update info (and writes `.zsync` if `zsyncmake` is installed), `-g` guesses it on GitHub Actions, `-s/--sign` signs with gpg, `--runtime-file` pins the runtime. | Downloads the latest type2-runtime unless `--runtime-file` is given. "In most cases you will be better off using one of the higher-level tools." It bundles no libraries. |
  | [`linuxdeploy`](https://docs.appimage.org/packaging-guide/from-source/linuxdeploy-user-guide.html) + [`linuxdeploy-plugin-appimage`](https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/blob/536b068787179ea901964bd7dabc7bf61e4941c3/README.md) | Builds the AppDir: `--executable`, `--desktop-file`, `--icon-file` (puts icons at the right hicolor paths), bundles `DT_NEEDED` libraries minus the excludelist, then `--output appimage`. | Env `LDAI_UPDATE_INFORMATION`, `LDAI_SIGN`, `LDAI_SIGN_KEY`, `LDAI_OUTPUT`, `LINUXDEPLOY_OUTPUT_VERSION`. |
  | [go-appimage `appimagetool`](https://github.com/probonopd/go-appimage/blob/b7864b5e53d4d7a1d5f31fdc23d1e591d77b8a21/README.md) | `deploy` verb bundles dependencies, and `-s deploy` bundles "EVERYTHING" including glibc ([src/appimagetool/README](https://github.com/probonopd/go-appimage/blob/b7864b5e53d4d7a1d5f31fdc23d1e591d77b8a21/src/appimagetool/README.md)). | On GitHub Actions it embeds `gh-releases-zsync|<owner>|<repo>|<release>|<file>.zsync` and writes the `.zsync` automatically. Maintained by the AppImage inventor, but the README calls it "experimental". |
  | [cargo-packager](https://github.com/crabnebula-dev/cargo-packager/blob/9adb8f9b94c60c848e93a668c8658b50c961c2b8/README.md) | Rust packager with an AppImage output (see `packaging-and-updates.md` §4). | Adds a tool and config; not needed for a single binary **(inference)**. |

  The AppImage docs page on GitHub Actions (`packaging-guide/hosted-services/github-actions.html`) returns 404 today.

### 2.2 What to bundle and what to leave to the system

- **The rule.** Every library dependency "MUST be included in the AppImage *IF* it cannot be assumed to be part of every target system in a recent enough version". [AppImageSpec, "The payload application"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#the-payload-application)
  - The AppImage team curates an **excludelist** of libraries "that we will assume to be present on the host system and hence should NOT be bundled". [pkg2appimage/excludelist @19e30b2](https://github.com/AppImageCommunity/pkg2appimage/blob/19e30b276ffedf4d3b4b56bc6320f463625a74f8/excludelist)
  - Bundling hardware-dependent libraries "might even break the AppImage", e.g. `libGL.so.1`. [AppImage concepts](https://docs.appimage.org/introduction/concepts.html)
- **Excludelist entries that cover everything we link or load:**
  - `libc.so.6`, `libm.so.6`, `libgcc_s.so.1`;
  - `libGL.so.1`, `libEGL.so.1`, `libGLX.so.0`, `libdrm.so.2`, `libgbm.so.1`;
  - `libX11.so.6`, `libxcb.so.1`, `libwayland-client.so.0` ("New version of Mesa has some dependency issues with libwayland-client if it is bundled");
  - `libasound.so.2`;
  - `libpipewire-0.3.so.0` (it must match the ABI of the system's server; see [linuxdeploy#292](https://github.com/linuxdeploy/linuxdeploy/issues/292)).

  `libvulkan.so.1` and `libxkbcommon.so.0` are **not** on the list. We `dlopen` both, so no deploy tool would pick them up anyway **(inference)**. Both are standard on Wayland desktops with a GPU driver **(inference)**.
- **Result for Das-Meter** **(inference)**. Nothing from `DT_NEEDED` needs bundling. The AppImage is the binary, `.desktop`, icons, MIME XML, the Send Plugin bundles (for **Install Send Plugin…**) and an `AppRun` symlink. linuxdeploy would bundle nothing extra. It remains useful mainly for placing icons and for its `--output appimage` step, so plain `appimagetool` on a scripted AppDir does the same job with one tool fewer.
- **glibc baseline.** "The binaries contained in the AppImage need to be compiled on a system not newer than the oldest base system that the AppImage is intended to run on." The docs suggest "the oldest still-supported LTS release of Ubuntu". [AppImage best practices](https://docs.appimage.org/reference/best-practices.html), [concepts](https://docs.appimage.org/introduction/concepts.html)
  - Ubuntu 22.04 ships glibc 2.35 and libpipewire 0.3.48. Ubuntu 24.04 ships glibc 2.39 and libpipewire 1.0.5. [packages.ubuntu.com: jammy libc6](https://packages.ubuntu.com/jammy/libc6), [noble libc6](https://packages.ubuntu.com/noble/libc6), [jammy libpipewire-0.3-dev](https://packages.ubuntu.com/jammy/libpipewire-0.3-dev), [noble libpipewire-0.3-dev](https://packages.ubuntu.com/noble/libpipewire-0.3-dev)
  - We need the `v0_3_53` API ([cpal Cargo.toml](https://github.com/RustAudio/cpal/blob/v0.18.2/Cargo.toml)), so building on 22.04 would need a newer PipeWire from elsewhere **(inference)**.
  - Staying on ubuntu-24.04 means the AppImage needs glibc ≥ 2.39 at most, which covers Arch, Fedora 40+ and Ubuntu 24.04+ **(inference; the exact floor is the highest `GLIBC_x.y` symbol version in the binary, which `objdump -T` shows)**.
- **Absolute paths.** The binary must not use compiled-in absolute paths, because the mount point changes every run. Resolve resources relative to `/proc/self/exe`. [AppImage best practices](https://docs.appimage.org/reference/best-practices.html) Das-Meter embeds its fonts (`crates/app/assets/fonts`), so this should already hold **(inference)**.

### 2.3 Update information (zsync)

- An AppImage **MAY** embed update information for exactly one transport, in the ELF section `.upd_info`. [AppImageSpec, "Update information"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#update-information)
  - For GitHub the form is `gh-releases-zsync|<user>|<repo>|<tag>|<file>.zsync`.
  - The tag field is `latest`, `latest-pre`, `latest-all` or a fixed tag, and the filename may contain `*`.
  - For us that would be: `gh-releases-zsync|daswerk|Das-Meter|latest|Das-Meter-*-x86_64.AppImage.zsync`.
  - `latest` skips prereleases, so the `-testN` dry runs would not be offered as updates **(inference from the spec's definitions)**.
- **How to embed it.** `appimagetool -u "<info>"`, which "if zsyncmake is installed" also writes the `.zsync`. With linuxdeploy, set `LDAI_UPDATE_INFORMATION`. Upload the `.zsync` next to the AppImage in the release. [AppImage docs: Making AppImages updateable](https://docs.appimage.org/packaging-guide/optional/updates.html), [appimagetool README](https://github.com/AppImage/appimagetool/blob/8c8c91f762b412a19f4e8d2c4b35afb98f2d7c81/README.md)
- **What it buys.** External tools can delta-update the file: AppImageUpdate/`appimageupdatetool`, and AppImageLauncher's "Update" launcher entry ([AppImageLauncher README](https://github.com/TheAssassin/AppImageLauncher/blob/edd4ef418ece70112970b74408818e77b40264bd/README.md)). This works independently of our own daily check and note **(inference)**. It costs one extra release asset.
- **The AppImage project's own "Golden Rules" for in-app updating** [updates page](https://docs.appimage.org/packaging-guide/optional/updates.html):
  - "Never download updates without the user's explicit consent".
  - Respect global "do not check" flags.
  - "Do not bother the user with updates directly as the first thing when the application is launched".
  - "Ask the user for permission before doing version checks".
  - Releases update to releases, nightlies to nightlies.

  The map's daily check should respect these, especially the permission point **(inference)**.

### 2.4 Signing

- **Embedded signature.** The spec allows a signature in the ELF section `.sha256_sig`, over the SHA-256 of the file with that section zeroed. [AppImageSpec, "Type 2 image format"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#type-2-image-format)
  - `appimagetool --sign` (with `--sign-key`, and `APPIMAGETOOL_SIGN_PASSPHRASE` for CI) signs with gpg. [appimagetool README](https://github.com/AppImage/appimagetool/blob/8c8c91f762b412a19f4e8d2c4b35afb98f2d7c81/README.md)
  - `--appimage-signature` prints the signature but "does not validate" it. Validating needs an external tool such as `validate` from AppImageUpdate. [AppImage docs: Signing AppImages](https://docs.appimage.org/packaging-guide/optional/signatures.html)
  - Nothing in a stock desktop checks it on launch **(inference)**.
- **Alternative.** Keep the existing **minisign** key (`MINISIGN_SECRET_KEY` in the `release` environment) and publish `Das-Meter-…AppImage.minisig`, plus `SHA256SUMS` **(inference)**. The in-app update note only links the release, so it has no signature to check. A future self-update would reuse the macOS minisign path.

### 2.5 Sketch: CI steps (ubuntu-24.04, after the existing build)

Not tested. It illustrates the pieces named above.

```bash
v=$(cargo pkgid -p dasmeter-app | sed 's/.*[#@]//')
app=AppDir
install -Dm755 target/release/das-meter            $app/usr/bin/das-meter
install -Dm644 packaging/linux/<app-id>.desktop     $app/usr/share/applications/<app-id>.desktop
install -Dm644 packaging/linux/<app-id>.svg         $app/usr/share/icons/hicolor/scalable/apps/<app-id>.svg
install -Dm644 packaging/linux/<app-id>-256.png     $app/usr/share/icons/hicolor/256x256/apps/<app-id>.png
install -Dm644 packaging/linux/<app-id>.xml         $app/usr/share/mime/packages/<app-id>.xml
cp -r "target/plugins/Das-Meter Send.clap" "target/plugins/Das-Meter Send.vst3" $app/usr/lib/das-meter/
ln -s usr/bin/das-meter $app/AppRun
ln -s usr/share/applications/<app-id>.desktop $app/<app-id>.desktop
ln -s usr/share/icons/hicolor/scalable/apps/<app-id>.svg $app/<app-id>.svg
ln -s usr/share/icons/hicolor/256x256/apps/<app-id>.png $app/.DirIcon
sudo apt-get install -y zsync          # provides zsyncmake
# appimagetool is itself an AppImage; extract-and-run avoids needing FUSE on the runner
APPIMAGE_EXTRACT_AND_RUN=1 VERSION=$v ./appimagetool-x86_64.AppImage \
  --runtime-file runtime-x86_64 \
  -u "gh-releases-zsync|daswerk|Das-Meter|latest|Das-Meter-*-x86_64.AppImage.zsync" \
  $app "Das-Meter-$v-x86_64.AppImage"
```

Pin the `appimagetool` and `runtime-x86_64` downloads by version and checksum. The type2-runtime releases are gpg-signed ([README](https://github.com/AppImage/type2-runtime/blob/75849dce7cc37e4319b633df1f116ca895c71a12/README.md)). The Arch rules on not diminishing source integrity make the same point for packages.

---

## 3. Desktop integration (both formats)

### 3.1 The `.desktop` file

- **Name and app ID.** [Desktop Entry Specification 1.5, "File naming"](https://specifications.freedesktop.org/desktop-entry-spec/latest/)
  - The file name "should follow the 'reverse DNS' convention", e.g. `org.example.FooViewer.desktop`.
  - The **desktop file ID** is its path below `applications/`.
  - Dashes are "allowed but not recommended"; if the domain has one, "replacing it with an underscore is recommended". The app part is conventionally CamelCase.
- **Wayland needs the match.** xdg-shell's `set_app_id`: "it is suggested to select app ID's that match the basename of the application's .desktop file". [xdg-shell.xml](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/xdg-shell/xdg-shell.xml)
  - winit sets it through `WindowAttributesExtWayland::with_name(general, _)`: "The `general` name sets an application ID, which should match the `.desktop` file". [winit platform/wayland.rs @v0.30.13](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform/wayland.rs)
  - On X11, the WM class plays that role, and `StartupWMClass=` in the desktop file names it. [Desktop Entry spec, "Recognized keys"](https://specifications.freedesktop.org/desktop-entry-spec/latest/)
  - Without the match, GNOME shows a generic icon and can't pin the running window to its launcher **(inference)**.
- **Choosing the ID** **(inference)**.
  - The macOS bundle ID is `com.daswerk.das-meter` (`scripts/macos-bundle.sh`). Following the spec's advice, the Linux ID would be e.g. **`com.daswerk.DasMeter`**.
  - The spec wants a domain "controlled by the author". If `daswerk.com` isn't ours, `io.github.daswerk.DasMeter` is the widely used form for GitHub-hosted projects **(unverified as a spec rule; it is a Flathub convention)**.
  - The same ID should name the icon, the MIME XML, the autostart file and the Wayland `app_id`.
- **Keys we'd use** [Desktop Entry spec, "Recognized desktop entry keys" and "The Exec key"](https://specifications.freedesktop.org/desktop-entry-spec/latest/):

  ```ini
  [Desktop Entry]
  Type=Application
  Name=Das-Meter
  Comment=Audio meters for your desktop
  Exec=das-meter %F
  Icon=com.daswerk.DasMeter
  Categories=AudioVideo;Audio;
  MimeType=application/x-dasmeter-preset;
  StartupWMClass=das-meter
  SingleMainWindow=true
  Keywords=meter;loudness;LUFS;spectrum;
  ```

  - `%f` is one file and `%F` a list of files. The app must be able to open the listed MIME types "using the command listed in the Exec key".
  - `SingleMainWindow` is a hint that the app has one main window.
  - `DBusActivatable` needs a D-Bus service; we don't have one, so leave it out.
  - Main category `AudioVideo` needs `Audio` or `Video` alongside it per the Desktop Menu spec. **Unverified**: I didn't open the Desktop Menu spec.
  - For the AppImage, appimagetool fills in `X-AppImage-Version` from `VERSION`. [AppImage docs: desktop integration](https://docs.appimage.org/reference/desktop-integration.html), [appimagetool README](https://github.com/AppImage/appimagetool/blob/8c8c91f762b412a19f4e8d2c4b35afb98f2d7c81/README.md)
  - `desktop-file-validate` (from `desktop-file-utils`) checks the file; running it in CI is cheap **(inference)**.

### 3.2 Icons (hicolor)

- "Minimally you should install a 48x48 icon in the hicolor theme … `$prefix/share/icons/hicolor/48x48/apps`." An SVG in `hicolor/scalable/apps` "means most desktops will have one icon that works for all sizes." [Icon Theme Specification 0.13, "Installing Application Icons"](https://specifications.freedesktop.org/icon-theme-spec/latest/)
- Implementations must fall back to `hicolor`. HiDPI variants go in `NxN@2` directories. [same](https://specifications.freedesktop.org/icon-theme-spec/latest/)
- Arch's `hicolor-icon-theme` defines these directories: 16, 22, 24, 32, 36, 48, 64, 72, 96, 128, 192, 256 and 512, each with an `@2` variant, plus `scalable` and `symbolic`. [hicolor-icon-theme file list](https://archlinux.org/packages/extra/any/hicolor-icon-theme/) The Arch `surge-xt-standalone` package installs PNGs at 16, 32, 128 and 256 (and more). [surge-xt-standalone](https://archlinux.org/packages/extra/x86_64/surge-xt-standalone/)
- AppImage wants icons under `usr/share/icons/hicolor` as well as a root icon named after `Icon=`, and a `.DirIcon`. If the root icon is a PNG, it "SHOULD be of size 256x256, 512x512, or 1024x1024". [AppImageSpec, "Contents of the image"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#contents-of-the-image)
- **A practical set** **(inference)**: `scalable/apps/<id>.svg` plus PNGs at 48, 128 and 256 (and 512 for the AppImage root icon). Optionally add `symbolic/apps/<id>-symbolic.svg` for GNOME's monochrome contexts, and a MIME icon for Preset files (§3.3).

### 3.3 MIME type for `.dasmeter-preset` (shared-mime-info)

- **How to register.** "Each application that wishes to contribute to the MIME database will install a single XML file, named after the application, into one of the three `<MIME>/packages/` directories." After installing, removing or changing it, "the application MUST run the `update-mime-database` command". [Shared MIME-info Database spec](https://specifications.freedesktop.org/shared-mime-info-spec/latest/)
  - `<MIME>` is `mime/` under each of `XDG_DATA_HOME` and `XDG_DATA_DIRS`, i.e. `/usr/share/mime/packages/` for the AUR package and `~/.local/share/mime/packages/` for an AppImage.
  - On Arch, pacman's `30-update-mime-database.hook` runs the update for packages ([shared-mime-info](https://archlinux.org/packages/extra/x86_64/shared-mime-info/)). An AppImage that installs into `~/.local` has to run `update-mime-database ~/.local/share/mime` itself **(inference)**.
- **Glob and icons.** A `<glob pattern="*.dasmeter-preset"/>` maps the extension; the default glob weight is 50. The file's icon comes from the MIME type name with `/` mapped to `-` (e.g. `application-x-dasmeter-preset`), unless `<icon name=…>` overrides it. `<generic-icon>` gives a fallback. [same spec](https://specifications.freedesktop.org/shared-mime-info-spec/latest/)
- **Content.** Preset files are TOML (`crates/core/src/presets.rs` serialises with `toml`). So `<sub-class-of type="text/plain"/>` lets text editors offer to open them **(inference)**.

  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
    <mime-type type="application/x-dasmeter-preset">
      <comment>Das-Meter Preset</comment>
      <sub-class-of type="text/plain"/>
      <glob pattern="*.dasmeter-preset"/>
      <icon name="com.daswerk.DasMeter"/>
    </mime-type>
  </mime-info>
  ```

  The type name `application/x-dasmeter-preset` is my choice. The spec doesn't dictate unregistered names. It parallels the macOS UTI `com.daswerk.das-meter.preset`.
- **Opening files.** The `.desktop` file's `MimeType=` plus `Exec=… %F` makes the app a handler. `update-desktop-database` rebuilds `mimeinfo.cache`; pacman's hook does it for packages, and an AppImage integrating into `~/.local/share/applications` must run it itself **(inference)**. Choosing the *default* handler is the user's or desktop's business (`mimeapps.list`, [MIME Applications Associations spec](https://specifications.freedesktop.org/mime-apps-spec/latest/)). With only one handler registered, it becomes the default in practice **(inference)**.

### 3.4 Launch at Login (XDG Autostart)

- "By placing an application's .desktop file in one of the Autostart directories the application will be automatically launched during startup of the user's desktop environment after the user has logged in." The directories are `$XDG_CONFIG_DIRS/autostart` and `$XDG_CONFIG_HOME/autostart`, i.e. `/etc/xdg/autostart/` and `~/.config/autostart/`. When the same name exists in both, the user's file wins. [Desktop Application Autostart Specification](https://specifications.freedesktop.org/autostart-spec/latest/)
  - `Hidden=true` in the user's file suppresses a same-named system file.
  - `OnlyShowIn`/`NotShowIn` limit it to certain desktops.
  - A non-empty `TryExec` that doesn't resolve stops the autostart. [same](https://specifications.freedesktop.org/autostart-spec/latest/)
- **For Das-Meter** **(inference)**:
  - The toggle writes or removes `~/.config/autostart/<app-id>.desktop`. Nothing goes into `/etc/xdg/autostart`, which would start the app for every user and needs root.
  - **AUR**: `Exec=das-meter --background` (or whatever flag starts the app without its window), with `TryExec=das-meter` so the entry goes quiet if the package is removed.
  - **AppImage**: `Exec="<value of $APPIMAGE>" …`, using the file's real path from the runtime. If the user moves or deletes the AppImage, the entry silently fails. The app can re-write the path on each launch while the toggle is on, and the settings panel can say the AppImage should stay where it is.
  - Autostart doesn't depend on a tray.
- **The portal alternative.** `org.freedesktop.portal.Background.RequestBackground` has an `autostart` option ("be started automatically at login") and a `commandline` option. [xdg-desktop-portal: Background](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Background.html) It is designed for sandboxed apps. Whether it works for unsandboxed host apps (AUR binary, AppImage) is **unverified**. Writing the file directly is the standard, dependency-free path.

### 3.5 AppImage first-run integration norms

- **The spec's rules.** The software inside an AppImage "MAY integrate into the desktop environment (e.g., by installing a .desktop file into the host system) on the user's behalf. However if it does so, it SHOULD ensure to get the explicit permission of the user." It "SHOULD NOT" integrate if any of these hold [AppImageSpec, "Desktop integration"](https://github.com/AppImage/AppImageSpec/blob/5220f3e9da0b6368da23105f80b0aff02c66af87/draft.md#desktop-integration):
  - `$XDG_DATA_HOME/appimagekit/no_desktopintegration`, `/usr/share/appimagekit/no_desktopintegration` or `/etc/appimagekit/no_desktopintegration` exists;
  - a process named `appimaged` is running;
  - `$DESKTOPINTEGRATION` is non-empty.
- **Third-party integrators users may already run.**
  - [`appimaged`](https://docs.appimage.org/user-guide/run-appimages.html#appimaged) watches directories and integrates AppImages automatically.
  - [AppImageLauncher](https://github.com/TheAssassin/AppImageLauncher/blob/edd4ef418ece70112970b74408818e77b40264bd/README.md) intercepts the first launch and asks "run once" or "integrate". Integrating moves the file to `~/Applications` and adds menu entries plus "Update" and "Remove" actions. These tools extract and patch the embedded desktop entry, so shipping a correct one inside the AppImage matters even if we never integrate ourselves.
- **What self-integration would write** **(inference; mirrors what the integrators do)**:
  - `~/.local/share/applications/<app-id>.desktop`, with `Exec=` and `TryExec=` set to `$APPIMAGE`;
  - icons into `~/.local/share/icons/hicolor/…`;
  - the MIME XML into `~/.local/share/mime/packages/`, then `update-mime-database ~/.local/share/mime` and `update-desktop-database ~/.local/share/applications`.

  Undoing it is the reverse. A "Remove from menu" button next to **Install Send Plugin…** would mirror the uninstall story.
- **Install Send Plugin…** copies the bundles from `$APPDIR/usr/lib/das-meter/` to `~/.clap/` and `~/.vst3/`, the user paths in both specs. [clap/entry.h](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/entry.h), [VST3 Plug-in Locations](https://github.com/steinbergmedia/vst3_dev_portal/blob/3dcb7be336478075ecb8baf1513dd399a93acd31/src/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BLocations.md) The copies are real files, so they keep working after the AppImage is unmounted. After an AppImage update, the plug-ins stay at the old version until the user installs them again **(inference)**.

---

## 4. What this means for the release (input to #86)

These are facts and inferences for the decision, not decisions.

1. **Release assets a `-bin` package and the AppImage both need:**
   - the Send Plugin built on Linux (`scripts/bundle-send-plugin.sh` handles macOS and Windows today; the Linux release job doesn't build it);
   - a `.desktop` file, icons and MIME XML, kept in the repo (e.g. `packaging/linux/`);
   - a tarball laid out so `package()` is a few `install`/`cp` lines;
   - a `SHA256SUMS` or b2 sums file.
2. **The AppImage:** `Das-Meter-<ver>-x86_64.AppImage`, not wrapped in an archive, with an optional `.zsync` and `gh-releases-zsync` update info.
3. **AUR publishing:**
   - Manual: the owner's account and key, `makepkg --printsrcinfo`, `git push`.
   - Automated: a CI key on the owner's AUR profile, the KSX action, and a PKGBUILD template filled with the version and checksum.

   Arch's warning about automation applies either way. A middle ground is CI opening a PR/branch with the updated PKGBUILD and the owner pushing it **(inference)**.
4. **Build host:** ubuntu-24.04 stays. It is the oldest runner with a new enough PipeWire; the glibc floor is 2.39.

## 5. Open points to check by hand

- Run `readelf -d` / `namcap` on a real build to confirm §1.4 (linked vs dlopened) before fixing `depends`.
- Check that winit's `with_name` app ID and the desktop file ID match. Then confirm on GNOME/Wayland that the running window gets the right icon and pins to the launcher.
- Confirm that Bitwig on Arch finds plug-ins in `/usr/lib/clap` and `/usr/lib/vst3` (the specs say it should) and in `~/.clap` and `~/.vst3`.
- Whether `daswerk.com` (or another domain) is ours, which decides `com.daswerk.DasMeter` vs `io.github.daswerk.DasMeter`.
