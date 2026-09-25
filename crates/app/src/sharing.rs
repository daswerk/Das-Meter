//! Export and import of `.dasmeter-preset` files through the OS's file dialogs.

use std::path::Path;

use dasmeter_core::sharing::EXTENSION;
use dasmeter_core::{AppCore, Event};

/// Asks where to save the current Preset, and writes it there.
pub fn export(core: &AppCore) {
    let Some((name, contents)) = core.export() else {
        return;
    };
    let file_name = format!("{name}.{EXTENSION}");
    let Some(path) = rfd::FileDialog::new()
        .set_title("Export Preset")
        .set_file_name(&file_name)
        .add_filter("Das-Meter Preset", &[EXTENSION])
        .save_file()
    else {
        return;
    };
    if let Err(error) = std::fs::write(&path, contents) {
        eprintln!("Das-Meter couldn't write {}: {error}", path.display());
    }
}

/// Asks for a file to import, and imports it.
pub fn import_chosen(core: &mut AppCore, now: std::time::Duration) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Import Preset")
        .add_filter("Das-Meter Preset", &[EXTENSION])
        .pick_file()
    else {
        return;
    };
    import(core, &path, now);
}

/// Whether a dropped or opened file is one to import.
pub fn is_preset(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXTENSION)
}

/// Imports the file at `path`. A file that can't be read or isn't a Preset
/// changes nothing (the core says so in a note).
pub fn import(core: &mut AppCore, path: &Path, now: std::time::Duration) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    core.handle(Event::ImportPreset(&text), now);
}
