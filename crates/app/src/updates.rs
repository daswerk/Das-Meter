//! Self-update on macOS (ADR 0004): updates bypass Gatekeeper, so the app
//! checks them itself.
//!
//! Once a day (if the setting is on) it asks GitHub Releases for the latest
//! release. **Update** downloads `Das-Meter-<version>.app.tar.gz`, verifies its
//! minisign (Ed25519) signature against the public key built into the app,
//! unpacks it next to the app, checks that the new app is signed with our
//! certificate, swaps it in by rename and relaunches. If the app's folder
//! isn't writable, the release page opens instead.
//!
//! The public key and the certificate's SHA-1 come in at build time
//! (`DASMETER_MINISIGN_PUBLIC_KEY`, `DASMETER_CERTIFICATE_SHA1`); a build
//! without them can check for updates but never installs one.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::plugin_install::Version;

/// The latest published release (drafts and pre-releases are left out).
pub const LATEST_RELEASE: &str = "https://api.github.com/repos/daswerk/Das-Meter/releases/latest";
/// How often to look.
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// The release signing key, as `minisign -G` prints it (the base64 line).
pub const PUBLIC_KEY: Option<&str> = option_env!("DASMETER_MINISIGN_PUBLIC_KEY");
/// The SHA-1 of the self-made code-signing certificate every release is signed with.
pub const CERTIFICATE_SHA1: Option<&str> = option_env!("DASMETER_CERTIFICATE_SHA1");

/// This app's version.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("a major.minor.patch package version")
}

/// A release that can be installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release's page, opened when the app can't update itself.
    pub page: String,
    pub tarball: String,
    pub signature: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Reads GitHub's answer for the latest release. `None` for a draft, a
/// pre-release, an odd tag, or one without the tarball and its signature.
pub fn parse_release(json: &str) -> Option<Release> {
    let release: GitHubRelease = serde_json::from_str(json).ok()?;
    if release.draft || release.prerelease {
        return None;
    }
    let version = Version::parse(&release.tag_name)?;
    let tarball_name = format!("Das-Meter-{version}.app.tar.gz");
    let signature_name = format!("{tarball_name}.minisig");
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    Some(Release {
        version,
        page: release.html_url.clone(),
        tarball: url(&tarball_name)?,
        signature: url(&signature_name)?,
    })
}

/// Why an update didn't happen.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// This build has no release key or certificate built in.
    NotConfigured,
    Download(String),
    /// The signature is missing, malformed, from another key, or doesn't match.
    BadSignature,
    Unpack(String),
    /// The unpacked app isn't signed with our certificate.
    NotOurCertificate,
    Swap(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::NotConfigured => write!(f, "this build can't install updates"),
            UpdateError::Download(e) => write!(f, "the download failed: {e}"),
            UpdateError::BadSignature => write!(f, "the download's signature didn't check out"),
            UpdateError::Unpack(e) => write!(f, "the download couldn't be unpacked: {e}"),
            UpdateError::NotOurCertificate => {
                write!(f, "the new app isn't signed with Das-Meter's certificate")
            }
            UpdateError::Swap(e) => write!(f, "the app couldn't be replaced: {e}"),
        }
    }
}

/// Checks `data` against a minisign signature file's text with `public_key`.
pub fn verify_signature(
    data: &[u8],
    signature: &str,
    public_key: Option<&str>,
) -> Result<(), UpdateError> {
    let key = public_key.ok_or(UpdateError::NotConfigured)?;
    let key = minisign_verify::PublicKey::from_base64(key.trim())
        .map_err(|_| UpdateError::NotConfigured)?;
    let signature =
        minisign_verify::Signature::decode(signature).map_err(|_| UpdateError::BadSignature)?;
    key.verify(data, &signature, false)
        .map_err(|_| UpdateError::BadSignature)
}

/// The code requirement a new app must meet: signed by our certificate.
pub fn signer_requirement(certificate_sha1: Option<&str>) -> Result<String, UpdateError> {
    let sha1 = certificate_sha1.ok_or(UpdateError::NotConfigured)?.trim();
    if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(UpdateError::NotConfigured);
    }
    Ok(format!(
        "certificate leaf = H\"{}\"",
        sha1.to_ascii_lowercase()
    ))
}

/// Whether `app` is validly signed and meets `requirement`.
pub fn check_signer(app: &Path, requirement: &str) -> Result<(), UpdateError> {
    let status = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(format!("-R={requirement}"))
        .arg(app)
        .output()
        .map_err(|_| UpdateError::NotOurCertificate)?;
    if status.status.success() {
        Ok(())
    } else {
        Err(UpdateError::NotOurCertificate)
    }
}

/// Downloads `url` with the system's curl.
pub fn download(url: &str) -> Result<Vec<u8>, UpdateError> {
    let output = Command::new("/usr/bin/curl")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--max-time", "300", "--proto", "=https"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .args([
            "--user-agent",
            concat!("Das-Meter/", env!("CARGO_PKG_VERSION")),
        ])
        .arg(url)
        .output()
        .map_err(|e| UpdateError::Download(e.to_string()))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(UpdateError::Download(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// The newer release, if there is one.
pub fn check() -> Result<Option<Release>, UpdateError> {
    let json = download(LATEST_RELEASE)?;
    let release = parse_release(&String::from_utf8_lossy(&json));
    Ok(release.filter(|r| r.version > current_version()))
}

/// Whether the app can replace itself: its folder is writable.
pub fn can_replace(app: &Path) -> bool {
    let Some(folder) = app.parent() else {
        return false;
    };
    let probe = folder.join(format!(".das-meter-write-test-{}", std::process::id()));
    let writable = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    writable
}

/// The running app's bundle, if it runs from one.
pub fn this_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Das-Meter.app/Contents/MacOS/das-meter
    let app = exe.parent()?.parent()?.parent()?;
    (app.extension()? == "app").then(|| app.to_path_buf())
}

/// Downloads, verifies and swaps in `release` over `app`. Returns the app to
/// relaunch.
pub fn install(release: &Release, app: &Path) -> Result<PathBuf, UpdateError> {
    let requirement = signer_requirement(CERTIFICATE_SHA1)?;
    if PUBLIC_KEY.is_none() {
        return Err(UpdateError::NotConfigured);
    }
    let tarball = download(&release.tarball)?;
    let signature = download(&release.signature)?;
    verify_signature(&tarball, &String::from_utf8_lossy(&signature), PUBLIC_KEY)?;
    let folder = app.parent().ok_or(UpdateError::Swap("no folder".into()))?;
    let staging = folder.join(format!(".Das-Meter-update-{}", std::process::id()));
    let result = unpack(&tarball, &staging).and_then(|new_app| {
        check_signer(&new_app, &requirement)?;
        swap(&new_app, app)
    });
    let _ = fs::remove_dir_all(&staging);
    result.map(|()| app.to_path_buf())
}

/// Unpacks the tarball into `staging` and returns the one `.app` in it.
fn unpack(tarball: &[u8], staging: &Path) -> Result<PathBuf, UpdateError> {
    let unpack = |e: std::io::Error| UpdateError::Unpack(e.to_string());
    let _ = fs::remove_dir_all(staging);
    fs::create_dir_all(staging).map_err(unpack)?;
    let archive = staging.join("update.tar.gz");
    fs::write(&archive, tarball).map_err(unpack)?;
    let status = Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(staging)
        .status()
        .map_err(unpack)?;
    if !status.success() {
        return Err(UpdateError::Unpack("tar failed".into()));
    }
    let apps: Vec<PathBuf> = fs::read_dir(staging)
        .map_err(unpack)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "app"))
        .collect();
    match apps.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(UpdateError::Unpack(
            "expected one app in the archive".into(),
        )),
    }
}

/// Puts `new_app` where `app` is, by renames: the old app is moved aside
/// first and put back if the new one can't move in.
pub fn swap(new_app: &Path, app: &Path) -> Result<(), UpdateError> {
    let swap = |e: std::io::Error| UpdateError::Swap(e.to_string());
    let folder = app.parent().ok_or(UpdateError::Swap("no folder".into()))?;
    let old = folder.join(format!(".Das-Meter-old-{}.app", std::process::id()));
    let _ = fs::remove_dir_all(&old);
    fs::rename(app, &old).map_err(swap)?;
    if let Err(error) = fs::rename(new_app, app) {
        let _ = fs::rename(&old, app);
        return Err(swap(error));
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

/// Opens a new instance of `app` once this one has quit.
pub fn relaunch(app: &Path) {
    // `open -n` would start it while this one still runs; wait for our pid.
    let script = format!(
        "while kill -0 {} 2>/dev/null; do sleep 0.2; done; /usr/bin/open \"$0\"",
        std::process::id()
    );
    let _ = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(app)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(tag: &str, prerelease: bool, assets: &[&str]) -> String {
        let assets: Vec<String> = assets
            .iter()
            .map(|name| {
                format!(
                    r#"{{"name":"{name}","browser_download_url":"https://example.invalid/{name}"}}"#
                )
            })
            .collect();
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/daswerk/Das-Meter/releases/tag/{tag}","draft":false,"prerelease":{prerelease},"assets":[{}]}}"#,
            assets.join(",")
        )
    }

    #[test]
    fn a_release_with_its_tarball_and_signature_is_read() {
        let json = release_json(
            "v1.2.0",
            false,
            &[
                "Das-Meter-1.2.0.dmg",
                "Das-Meter-1.2.0.app.tar.gz",
                "Das-Meter-1.2.0.app.tar.gz.minisig",
            ],
        );
        let release = parse_release(&json).unwrap();
        assert_eq!(release.version, Version(1, 2, 0));
        assert!(release.tarball.ends_with("Das-Meter-1.2.0.app.tar.gz"));
        assert!(release.signature.ends_with(".minisig"));
        assert!(release.page.ends_with("/tag/v1.2.0"));
    }

    #[test]
    fn pre_releases_and_incomplete_releases_are_skipped() {
        let full = [
            "Das-Meter-1.2.0.app.tar.gz",
            "Das-Meter-1.2.0.app.tar.gz.minisig",
        ];
        assert_eq!(parse_release(&release_json("v1.2.0", true, &full)), None);
        assert_eq!(
            parse_release(&release_json("v1.2.0", false, &full[..1])),
            None,
            "no signature"
        );
        assert_eq!(parse_release(&release_json("nightly", false, &full)), None);
        assert_eq!(parse_release("not json"), None);
    }

    struct Keys {
        keypair: minisign::KeyPair,
    }

    impl Keys {
        fn new() -> Keys {
            Keys {
                keypair: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
            }
        }

        fn public(&self) -> String {
            self.keypair.pk.to_base64()
        }

        fn sign(&self, data: &[u8]) -> String {
            minisign::sign(Some(&self.keypair.pk), &self.keypair.sk, data, None, None)
                .unwrap()
                .into_string()
        }
    }

    #[test]
    fn a_good_signature_passes() {
        let keys = Keys::new();
        let data = b"Das-Meter.app.tar.gz";
        assert_eq!(
            verify_signature(data, &keys.sign(data), Some(&keys.public())),
            Ok(())
        );
    }

    #[test]
    fn signature_failures_are_refused() {
        let keys = Keys::new();
        let other = Keys::new();
        let data = b"Das-Meter.app.tar.gz";
        let signature = keys.sign(data);
        assert_eq!(
            verify_signature(b"tampered", &signature, Some(&keys.public())),
            Err(UpdateError::BadSignature),
            "changed data"
        );
        assert_eq!(
            verify_signature(data, &signature, Some(&other.public())),
            Err(UpdateError::BadSignature),
            "another key"
        );
        assert_eq!(
            verify_signature(data, "not a signature", Some(&keys.public())),
            Err(UpdateError::BadSignature)
        );
        assert_eq!(
            verify_signature(data, &signature, None),
            Err(UpdateError::NotConfigured),
            "no key built in"
        );
        assert_eq!(
            verify_signature(data, &signature, Some("garbage")),
            Err(UpdateError::NotConfigured)
        );
    }

    #[test]
    fn the_signer_requirement_needs_a_certificate_hash() {
        assert_eq!(signer_requirement(None), Err(UpdateError::NotConfigured));
        assert_eq!(
            signer_requirement(Some("abc")),
            Err(UpdateError::NotConfigured)
        );
        let sha1 = "0123456789ABCDEF0123456789abcdef01234567";
        assert_eq!(
            signer_requirement(Some(sha1)).unwrap(),
            "certificate leaf = H\"0123456789abcdef0123456789abcdef01234567\""
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_app_not_signed_with_our_certificate_is_refused() {
        let dir = std::env::temp_dir().join(format!("dasmeter-signer-{}", std::process::id()));
        let app = dir.join("Fake.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        let requirement =
            signer_requirement(Some("0123456789abcdef0123456789abcdef01234567")).unwrap();
        assert_eq!(
            check_signer(&app, &requirement),
            Err(UpdateError::NotOurCertificate)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn swapping_replaces_the_app_by_rename() {
        let dir = std::env::temp_dir().join(format!("dasmeter-swap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let app = dir.join("Das-Meter.app");
        let new_app = dir.join("staging/Das-Meter.app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("version"), "old").unwrap();
        fs::create_dir_all(&new_app).unwrap();
        fs::write(new_app.join("version"), "new").unwrap();
        swap(&new_app, &app).unwrap();
        assert_eq!(fs::read_to_string(app.join("version")).unwrap(), "new");
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 2, "no old copy left: {left:?}");
        // A missing new app leaves the old one where it was.
        assert!(swap(&dir.join("missing.app"), &app).is_err());
        assert_eq!(fs::read_to_string(app.join("version")).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }
}
