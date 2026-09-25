//! The presets folder and `settings.toml` on disk: read at launch, and the
//! app core's Preset operations carried out.

use std::path::{Path, PathBuf};

use dasmeter_core::presets::SETTINGS_FILE;
use dasmeter_core::{PresetFile, PresetOp};

/// Das-Meter's own folder: `~/Library/Application Support/Das-Meter` on
/// macOS, `%APPDATA%\Das-Meter` on Windows.
pub fn app_folder() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    };
    Some(base.join("Das-Meter"))
}

pub fn folder() -> Option<PathBuf> {
    Some(app_folder()?.join("presets"))
}

/// Every Preset file in the folder, and `settings.toml` if there is one.
pub fn read() -> (Vec<PresetFile>, Option<String>) {
    let settings =
        app_folder().and_then(|dir| std::fs::read_to_string(dir.join(SETTINGS_FILE)).ok());
    let Some(Ok(entries)) = folder().map(std::fs::read_dir) else {
        return (Vec::new(), settings);
    };
    let mut files: Vec<PresetFile> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .filter_map(|path| {
            Some(PresetFile {
                file_name: path.file_name()?.to_string_lossy().into_owned(),
                // An unreadable file still shows in the list, as "can't be read".
                text: std::fs::read_to_string(&path).unwrap_or_default(),
            })
        })
        .collect();
    files.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    (files, settings)
}

/// A name the core gave never leaves the folder.
fn safe(name: &str) -> bool {
    !name.contains(['/', '\\']) && !name.starts_with('.') && !name.is_empty()
}

/// Writes by a temporary file and a rename, so a file is never half written.
fn write(path: &Path, contents: &str) {
    let Some(dir) = path.parent() else { return };
    if let Err(error) = std::fs::create_dir_all(dir) {
        eprintln!("Das-Meter couldn't make {}: {error}", dir.display());
        return;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = dir.join(format!(".{name}.tmp"));
    let result =
        std::fs::write(&temporary, contents).and_then(|()| std::fs::rename(&temporary, path));
    if let Err(error) = result {
        eprintln!("Das-Meter couldn't save {}: {error}", path.display());
    }
}

/// Moves a file to the Trash (on macOS, `~/.Trash`), never deleting it.
fn trash(path: &Path) {
    let Some(name) = path.file_name() else { return };
    let trash = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Trash"))
    } else {
        // Until the Windows shell uses the Recycle Bin: a folder beside the Presets.
        path.parent().map(|dir| dir.join(".trash"))
    };
    let Some(trash) = trash else { return };
    let _ = std::fs::create_dir_all(&trash);
    let mut target = trash.join(name);
    let mut n = 2;
    while target.exists() {
        target = trash.join(format!("{} {n}", name.to_string_lossy()));
        n += 1;
    }
    if let Err(error) = std::fs::rename(path, &target) {
        eprintln!(
            "Das-Meter couldn't move {} to the Trash: {error}",
            path.display()
        );
    }
}

/// Carries out the core's Preset operations.
pub fn apply(ops: Vec<PresetOp>) {
    let (Some(app), Some(presets)) = (app_folder(), folder()) else {
        return;
    };
    for op in ops {
        match op {
            PresetOp::Write {
                file_name,
                contents,
            } if safe(&file_name) => write(&presets.join(file_name), &contents),
            PresetOp::WriteIfMissing {
                file_name,
                contents,
            } if safe(&file_name) => {
                let path = presets.join(file_name);
                if !path.exists() {
                    write(&path, &contents);
                }
            }
            PresetOp::Trash { file_name } if safe(&file_name) => trash(&presets.join(file_name)),
            PresetOp::Settings { contents } => write(&app.join(SETTINGS_FILE), &contents),
            _ => {}
        }
    }
}

/// Shows a Preset file in Finder (Explorer on Windows).
pub fn show_in_folder(file_name: &str) {
    let Some(dir) = folder() else { return };
    let path = dir.join(file_name);
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
    } else {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    };
    if let Err(error) = result {
        eprintln!("Das-Meter couldn't show {}: {error}", path.display());
    }
}
