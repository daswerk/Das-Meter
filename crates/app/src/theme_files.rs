//! The themes folder on disk: read into the app core, written from its edits.

use std::path::PathBuf;
use std::time::SystemTime;

use dasmeter_core::{FileWrite, ThemeFile};

/// Where Themes live: `themes` in Das-Meter's own folder
/// ([`app_folder`](crate::preset_files::app_folder)).
pub fn folder() -> Option<PathBuf> {
    Some(crate::preset_files::app_folder()?.join("themes"))
}

/// What the folder held when last read: each file's name, size and time.
pub type Signature = Vec<(String, u64, Option<SystemTime>)>;

fn theme_paths() -> Vec<PathBuf> {
    let Some(dir) = folder() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    paths.sort();
    paths
}

/// The folder's signature, to tell cheaply whether it changed.
pub fn signature() -> Signature {
    theme_paths()
        .into_iter()
        .map(|path| {
            let meta = std::fs::metadata(&path).ok();
            (
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                meta.as_ref().map_or(0, |m| m.len()),
                meta.and_then(|m| m.modified().ok()),
            )
        })
        .collect()
}

/// Every Theme file in the folder.
pub fn read() -> Vec<ThemeFile> {
    theme_paths()
        .into_iter()
        .filter_map(|path| {
            Some(ThemeFile {
                file_name: path.file_name()?.to_string_lossy().into_owned(),
                text: std::fs::read_to_string(&path).ok()?,
            })
        })
        .collect()
}

/// Writes files the core asked for: to a temporary file first, then renamed
/// over the old one, so a Theme is never half written.
pub fn write(writes: &[FileWrite]) {
    let Some(dir) = folder() else { return };
    if writes.is_empty() {
        return;
    }
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "Das-Meter couldn't make the themes folder {}: {error}",
            dir.display()
        );
        return;
    }
    for write in writes {
        // The name comes from the core, but never leaves the folder.
        if write.file_name.contains(['/', '\\']) || write.file_name.starts_with('.') {
            continue;
        }
        let path = dir.join(&write.file_name);
        let temporary = dir.join(format!(".{}.tmp", write.file_name));
        let result = std::fs::write(&temporary, &write.contents)
            .and_then(|()| std::fs::rename(&temporary, &path));
        if let Err(error) = result {
            eprintln!("Das-Meter couldn't save {}: {error}", path.display());
        }
    }
}
