//! The Send Plugin installer (ADR 0004): the app carries the Send Plugin as
//! CLAP, VST3 and AU inside its bundle and copies the formats the user picks
//! into the per-user plug-in folders, so no admin password is needed and the
//! copies carry no quarantine flag.
//!
//! Every copy goes to a temporary name next to its target first and is then
//! renamed into place, so a DAW never sees a half-written bundle. At each
//! launch, formats that are installed and older than the bundled ones are
//! replaced the same way; formats the user didn't install are never added.
//!
//! Everything here works on paths it is given, so tests run in a temp dir.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The Send Plugin's bundle name, without its extension.
pub const PLUGIN_NAME: &str = "Das-Meter Send";

/// A plug-in format the app installs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    Clap,
    Vst3,
    Au,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Clap, Format::Vst3, Format::Au];

    /// The folder under `~/Library/Audio/Plug-Ins` that DAWs scan.
    pub fn folder(self) -> &'static str {
        match self {
            Format::Clap => "CLAP",
            Format::Vst3 => "VST3",
            Format::Au => "Components",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Format::Clap => "clap",
            Format::Vst3 => "vst3",
            Format::Au => "component",
        }
    }

    /// The bundle's file name, e.g. `Das-Meter Send.vst3`.
    pub fn bundle_name(self) -> String {
        format!("{PLUGIN_NAME}.{}", self.extension())
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Clap => "CLAP",
            Format::Vst3 => "VST3",
            Format::Au => "AU",
        }
    }
}

/// A `major.minor.patch` version, compared numerically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Parses `1.2.3`, `v1.2.3`, `1.2` (patch 0) or `1` (minor and patch 0).
    /// Anything after a `-` or `+` (pre-release, build) is ignored.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let core = text.split(['-', '+']).next()?;
        let mut parts = core.split('.');
        let mut next = || -> Option<u32> {
            match parts.next() {
                Some(part) => part.parse().ok(),
                None => Some(0),
            }
        };
        let version = Version(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A bundle's version: `CFBundleShortVersionString` from its `Info.plist`.
pub fn bundle_version(bundle: &Path) -> Option<Version> {
    let plist = fs::read_to_string(bundle.join("Contents/Info.plist")).ok()?;
    plist_string(&plist, "CFBundleShortVersionString").and_then(Version::parse)
}

/// The `<string>` after `<key>name</key>` in an XML property list.
fn plist_string<'a>(plist: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("<key>{name}</key>");
    let rest = &plist[plist.find(&key)? + key.len()..];
    let rest = rest.trim_start().strip_prefix("<string>")?;
    Some(&rest[..rest.find("</string>")?])
}

/// Where the bundled and the installed plug-ins are.
#[derive(Clone, Debug)]
pub struct Places {
    /// The folder in the app bundle holding the three bundles
    /// (`Das-Meter.app/Contents/PlugIns`).
    pub bundled: PathBuf,
    /// `~/Library/Audio/Plug-Ins`.
    pub plug_ins: PathBuf,
}

impl Places {
    /// The places for the running app, if it runs from a bundle.
    pub fn for_this_app() -> Option<Places> {
        let exe = std::env::current_exe().ok()?;
        // Das-Meter.app/Contents/MacOS/das-meter
        let contents = exe.parent()?.parent()?;
        let bundled = contents.join("PlugIns");
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(Places {
            bundled,
            plug_ins: home.join("Library/Audio/Plug-Ins"),
        })
    }

    pub fn bundled(&self, format: Format) -> PathBuf {
        self.bundled.join(format.bundle_name())
    }

    pub fn installed(&self, format: Format) -> PathBuf {
        self.plug_ins
            .join(format.folder())
            .join(format.bundle_name())
    }

    /// The formats installed in the per-user folders.
    pub fn installed_formats(&self) -> Vec<Format> {
        Format::ALL
            .into_iter()
            .filter(|&f| self.installed(f).exists())
            .collect()
    }
}

/// What installing one format did.
#[derive(Debug)]
pub enum Outcome {
    Installed,
    /// It failed, naming the folder it couldn't write to.
    Failed {
        format: Format,
        folder: PathBuf,
        error: io::Error,
    },
}

/// Installs `formats` from the bundle into the per-user folders, replacing
/// what's there. Each format succeeds or fails on its own.
pub fn install(places: &Places, formats: &[Format]) -> Vec<Outcome> {
    formats
        .iter()
        .map(|&format| {
            let folder = places.plug_ins.join(format.folder());
            match replace(&places.bundled(format), &places.installed(format)) {
                Ok(()) => Outcome::Installed,
                Err(error) => Outcome::Failed {
                    format,
                    folder,
                    error,
                },
            }
        })
        .collect()
}

/// At launch: replaces each installed format that's older than the bundled
/// one. Returns the formats it updated, so the app can say "restart your DAW"
/// once; formats that aren't installed are left alone.
pub fn update_installed(places: &Places) -> io::Result<Vec<Format>> {
    let mut updated = Vec::new();
    for format in places.installed_formats() {
        let Some(bundled) = bundle_version(&places.bundled(format)) else {
            continue;
        };
        // An installed bundle without a readable version is older.
        let installed = bundle_version(&places.installed(format));
        if installed.is_none_or(|v| v < bundled) {
            replace(&places.bundled(format), &places.installed(format))?;
            updated.push(format);
        }
    }
    Ok(updated)
}

/// Copies the bundle at `from` to `to` by a temp copy and renames: never
/// writing into the bundle a DAW may have loaded.
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    if !from.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} isn't in the app", from.display()),
        ));
    }
    let folder = to.parent().expect("a plug-in folder");
    fs::create_dir_all(folder)?;
    let name = to.file_name().expect("a bundle name").to_string_lossy();
    let pid = std::process::id();
    let fresh = folder.join(format!(".{name}.new-{pid}"));
    let old = folder.join(format!(".{name}.old-{pid}"));
    let _ = fs::remove_dir_all(&fresh);
    if let Err(error) = copy_dir(from, &fresh) {
        let _ = fs::remove_dir_all(&fresh);
        return Err(error);
    }
    // A directory can't be renamed over a non-empty one: move the old one
    // aside first, then the new one in, then remove the old.
    let had_old = to.exists();
    if had_old {
        let _ = fs::remove_dir_all(&old);
        if let Err(error) = fs::rename(to, &old) {
            let _ = fs::remove_dir_all(&fresh);
            return Err(error);
        }
    }
    if let Err(error) = fs::rename(&fresh, to) {
        if had_old {
            let _ = fs::rename(&old, to);
        }
        let _ = fs::remove_dir_all(&fresh);
        return Err(error);
    }
    if had_old {
        let _ = fs::remove_dir_all(&old);
    }
    Ok(())
}

/// Copies a directory tree, keeping symlinks as links (bundles may have them).
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, &target)?;
            #[cfg(not(unix))]
            fs::copy(entry.path(), &target).map(|_| ())?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// What Uninstall removes: the app, the Send Plugin in every per-user
/// folder, and the app's own folder (Presets, Themes, `settings.toml`) only
/// when the user ticked "Also delete my Presets and Themes".
pub fn uninstall_paths(
    app: &Path,
    places: &Places,
    user_data: &Path,
    also_user_data: bool,
) -> Vec<PathBuf> {
    let mut paths = vec![app.to_path_buf()];
    paths.extend(
        places
            .installed_formats()
            .into_iter()
            .map(|f| places.installed(f)),
    );
    if also_user_data && user_data.exists() {
        paths.push(user_data.to_path_buf());
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under the system temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let path = std::env::temp_dir()
                .join(format!("dasmeter-install-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_bundle(path: &Path, version: &str, payload: &str) {
        fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
        fs::write(
            path.join("Contents/Info.plist"),
            format!(
                "<plist><dict>\n    <key>CFBundleShortVersionString</key><string>{version}</string>\n</dict></plist>"
            ),
        )
        .unwrap();
        fs::write(path.join("Contents/MacOS").join(PLUGIN_NAME), payload).unwrap();
    }

    fn payload(bundle: &Path) -> String {
        fs::read_to_string(bundle.join("Contents/MacOS").join(PLUGIN_NAME)).unwrap()
    }

    fn places(dir: &TempDir, bundled_version: &str) -> Places {
        let places = Places {
            bundled: dir.0.join("Das-Meter.app/Contents/PlugIns"),
            plug_ins: dir.0.join("Plug-Ins"),
        };
        for format in Format::ALL {
            write_bundle(&places.bundled(format), bundled_version, "new");
        }
        places
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(Version::parse("1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::parse("v0.10.0"), Some(Version(0, 10, 0)));
        assert_eq!(Version::parse("1.2"), Some(Version(1, 2, 0)));
        assert_eq!(Version::parse("2.0.0-beta.1"), Some(Version(2, 0, 0)));
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("one"), None);
        assert_eq!(Version::parse(""), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert!(Version(1, 0, 0) > Version(0, 99, 99));
    }

    #[test]
    fn it_installs_the_chosen_formats() {
        let dir = TempDir::new("chosen");
        let places = places(&dir, "1.0.0");
        let outcomes = install(&places, &[Format::Clap, Format::Au]);
        assert!(outcomes.iter().all(|o| matches!(o, Outcome::Installed)));
        assert_eq!(places.installed_formats(), [Format::Clap, Format::Au]);
        assert!(
            places
                .plug_ins
                .join("Components/Das-Meter Send.component/Contents/Info.plist")
                .exists()
        );
    }

    #[test]
    fn a_folder_it_cant_write_fails_on_its_own() {
        let dir = TempDir::new("fails");
        let places = places(&dir, "1.0.0");
        // A file where the VST3 folder should be.
        fs::create_dir_all(&places.plug_ins).unwrap();
        fs::write(places.plug_ins.join("VST3"), "").unwrap();
        let outcomes = install(&places, &Format::ALL);
        let failed: Vec<_> = outcomes
            .iter()
            .filter_map(|o| match o {
                Outcome::Failed { format, folder, .. } => Some((*format, folder.clone())),
                Outcome::Installed => None,
            })
            .collect();
        assert_eq!(failed, [(Format::Vst3, places.plug_ins.join("VST3"))]);
        assert_eq!(places.installed_formats(), [Format::Clap, Format::Au]);
    }

    #[test]
    fn older_installed_formats_are_replaced_and_missing_ones_never_added() {
        let dir = TempDir::new("older");
        let places = places(&dir, "1.2.0");
        write_bundle(&places.installed(Format::Clap), "1.1.9", "old");
        write_bundle(&places.installed(Format::Vst3), "1.3.0", "newer");
        let updated = update_installed(&places).unwrap();
        assert_eq!(updated, [Format::Clap]);
        assert_eq!(payload(&places.installed(Format::Clap)), "new");
        assert_eq!(
            bundle_version(&places.installed(Format::Clap)),
            Some(Version(1, 2, 0))
        );
        assert_eq!(
            payload(&places.installed(Format::Vst3)),
            "newer",
            "newer stays"
        );
        assert!(!places.installed(Format::Au).exists(), "never added");
        assert_eq!(update_installed(&places).unwrap(), [], "once is enough");
    }

    #[test]
    fn a_same_version_is_left_alone() {
        let dir = TempDir::new("same");
        let places = places(&dir, "1.0.0");
        write_bundle(&places.installed(Format::Au), "1.0.0", "installed");
        assert_eq!(update_installed(&places).unwrap(), []);
        assert_eq!(payload(&places.installed(Format::Au)), "installed");
    }

    #[test]
    fn an_installed_bundle_without_a_version_is_replaced() {
        let dir = TempDir::new("unversioned");
        let places = places(&dir, "1.0.0");
        fs::create_dir_all(places.installed(Format::Vst3)).unwrap();
        assert_eq!(update_installed(&places).unwrap(), [Format::Vst3]);
    }

    #[test]
    fn replacing_renames_over_and_leaves_no_temp_files() {
        let dir = TempDir::new("rename");
        let places = places(&dir, "2.0.0");
        write_bundle(&places.installed(Format::Clap), "1.0.0", "old");
        // An extra file only the old bundle had must not survive.
        fs::write(places.installed(Format::Clap).join("Contents/stale"), "").unwrap();
        update_installed(&places).unwrap();
        let clap = places.installed(Format::Clap);
        assert!(!clap.join("Contents/stale").exists());
        let left: Vec<_> = fs::read_dir(clap.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["Das-Meter Send.clap"]);
    }

    #[test]
    fn uninstall_keeps_user_data_unless_asked() {
        let dir = TempDir::new("uninstall");
        let places = places(&dir, "1.0.0");
        install(&places, &[Format::Vst3]);
        let app = dir.0.join("Das-Meter.app");
        let data = dir.0.join("Application Support/Das-Meter");
        fs::create_dir_all(&data).unwrap();
        assert_eq!(
            uninstall_paths(&app, &places, &data, false),
            [app.clone(), places.installed(Format::Vst3)]
        );
        assert_eq!(
            uninstall_paths(&app, &places, &data, true),
            [app, places.installed(Format::Vst3), data]
        );
    }
}
