//! The macOS shell's upkeep of its own install (ADR 0004): refreshing the
//! installed Send Plugin at launch, the daily update check and Update, and
//! Uninstall. The file work is in [`crate::plugin_install`] and
//! [`crate::updates`]; this wires it to the menu, dialogs and notes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dasmeter_core::{AppCore, Event, Note};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::plugin_install::{self, Format, Outcome, Places};
use crate::updates::{self, Release, UpdateError};

/// From the worker threads to the main thread.
enum Message {
    /// The check finished: a newer release, or none. `asked` when the user
    /// chose Check for Updates (so "up to date" is said out loud).
    Checked {
        release: Option<Release>,
        asked: bool,
        failed: Option<String>,
    },
    /// The update is in place: relaunch this app.
    Installed(PathBuf),
    UpdateFailed(UpdateError),
}

pub struct Maintenance {
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: mpsc::Sender<Message>,
    messages: mpsc::Receiver<Message>,
    checking: bool,
    installing: bool,
    /// The release the last check found.
    available: Option<Release>,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Maintenance {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Maintenance {
        let (sender, messages) = mpsc::channel();
        Maintenance {
            wake,
            sender,
            messages,
            checking: false,
            installing: false,
            available: None,
        }
    }

    /// At launch: replaces any installed Send Plugin format that's older than
    /// the one in this app, and says so once.
    pub fn at_launch(&mut self, core: &mut AppCore, now: Duration) {
        let Some(places) = Places::for_this_app() else {
            return;
        };
        match plugin_install::update_installed(&places) {
            Ok(updated) if !updated.is_empty() => {
                core.handle(Event::ShowNote(Note::SendPluginUpdated), now);
            }
            Ok(_) => {}
            Err(error) => eprintln!("Das-Meter couldn't update the Send Plugin: {error}"),
        }
    }

    /// The release a check found, for the menu.
    pub fn available(&self) -> Option<&Release> {
        self.available.as_ref()
    }

    /// Starts the daily check when it's due (and the setting is on).
    pub fn check_if_due(&mut self, core: &mut AppCore, now: Duration) {
        if !core.app_settings().check_for_updates || self.checking {
            return;
        }
        let last = core.last_update_check();
        let due = unix_now().saturating_sub(last) >= updates::CHECK_EVERY.as_secs();
        if due {
            core.handle(Event::UpdateChecked { at: unix_now() }, now);
            self.check(false);
        }
    }

    /// Check for Updates… from the menu.
    pub fn check_now(&mut self, core: &mut AppCore, now: Duration) {
        if self.checking {
            return;
        }
        core.handle(Event::UpdateChecked { at: unix_now() }, now);
        self.check(true);
    }

    fn check(&mut self, asked: bool) {
        self.checking = true;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        thread::spawn(move || {
            let (release, failed) = match updates::check() {
                Ok(release) => (release, None),
                Err(error) => (None, Some(error.to_string())),
            };
            let _ = sender.send(Message::Checked {
                release,
                asked,
                failed,
            });
            wake();
        });
    }

    /// Update… from the menu: installs the release found, or opens its page
    /// when the app can't replace itself.
    pub fn update(&mut self, open_url: impl Fn(&str)) {
        let (Some(release), false) = (self.available.clone(), self.installing) else {
            return;
        };
        let Some(app) = updates::this_app() else {
            open_url(&release.page);
            return;
        };
        if !updates::can_replace(&app) {
            // Not writable (e.g. /Applications for a standard user): the user updates by hand.
            open_url(&release.page);
            return;
        }
        let answer = MessageDialog::new()
            .set_title("Update Das-Meter")
            .set_description(format!(
                "Download Das-Meter {} and restart with it? Your Presets and Themes stay as they are.",
                release.version
            ))
            .set_buttons(MessageButtons::OkCancelCustom("Update".into(), "Not Now".into()))
            .show();
        if answer != MessageDialogResult::Custom("Update".into()) {
            return;
        }
        self.installing = true;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        thread::spawn(move || {
            let message = match updates::install(&release, &app) {
                Ok(app) => Message::Installed(app),
                Err(error) => Message::UpdateFailed(error),
            };
            let _ = sender.send(message);
            wake();
        });
    }

    /// Handles what the workers finished. Returns true when the app should
    /// quit (an update is in place and relaunches).
    pub fn poll(&mut self, core: &mut AppCore, now: Duration) -> bool {
        let mut quit = false;
        while let Ok(message) = self.messages.try_recv() {
            match message {
                Message::Checked {
                    release,
                    asked,
                    failed,
                } => {
                    self.checking = false;
                    let found = release.is_some() && release != self.available;
                    self.available = release;
                    if found {
                        core.handle(Event::ShowNote(Note::UpdateAvailable), now);
                    }
                    if asked {
                        self.say_checked(failed);
                    }
                }
                Message::Installed(app) => {
                    updates::relaunch(&app);
                    quit = true;
                }
                Message::UpdateFailed(error) => {
                    self.installing = false;
                    eprintln!("Das-Meter couldn't update: {error}");
                    core.handle(Event::ShowNote(Note::UpdateFailed), now);
                }
            }
        }
        quit
    }

    fn say_checked(&self, failed: Option<String>) {
        let (level, description) = match (&self.available, failed) {
            (Some(release), _) => (
                MessageLevel::Info,
                format!(
                    "Das-Meter {} is out. Choose Das-Meter ▸ Update to Das-Meter {}… to install it.",
                    release.version, release.version
                ),
            ),
            (None, Some(error)) => (
                MessageLevel::Warning,
                format!("Das-Meter couldn't look for updates: {error}"),
            ),
            (None, None) => (
                MessageLevel::Info,
                format!(
                    "Das-Meter {} is the newest version.",
                    updates::current_version()
                ),
            ),
        };
        MessageDialog::new()
            .set_level(level)
            .set_title("Check for Updates")
            .set_description(description)
            .set_buttons(MessageButtons::Ok)
            .show();
    }
}

/// Install Send Plugin…: copies all three formats into the per-user folders
/// and says how it went. (The first-launch ticket puts a sheet with a choice
/// of formats on top of this.)
pub fn install_send_plugin() {
    let Some(places) = Places::for_this_app().filter(|p| p.bundled.is_dir()) else {
        MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Install Send Plugin")
            .set_description(
                "This Das-Meter doesn't carry the Send Plugin: run it from Das-Meter.app.",
            )
            .set_buttons(MessageButtons::Ok)
            .show();
        return;
    };
    let outcomes = plugin_install::install(&places, &Format::ALL);
    let failed: Vec<String> = outcomes
        .iter()
        .filter_map(|o| match o {
            Outcome::Failed {
                format,
                folder,
                error,
            } => Some(format!(
                "{}: {} ({error})",
                format.label(),
                folder.display()
            )),
            Outcome::Installed => None,
        })
        .collect();
    let (level, description) = if failed.is_empty() {
        (
            MessageLevel::Info,
            "Installed. Rescan plug-ins in your DAW (or restart it), then add Das-Meter Send to a track."
                .to_string(),
        )
    } else {
        (
            MessageLevel::Warning,
            format!("These folders couldn't be written:\n{}", failed.join("\n")),
        )
    };
    let dialog = MessageDialog::new()
        .set_level(level)
        .set_title("Install Send Plugin")
        .set_description(description);
    if failed.is_empty() {
        dialog.set_buttons(MessageButtons::Ok).show();
    } else if dialog
        .set_buttons(MessageButtons::OkCancelCustom(
            "Show in Finder".into(),
            "OK".into(),
        ))
        .show()
        == MessageDialogResult::Custom("Show in Finder".into())
    {
        let _ = std::process::Command::new("/usr/bin/open")
            .arg(&places.plug_ins)
            .spawn();
    }
}

/// Uninstall Das-Meter…: after confirming, moves the app and the per-user
/// Send Plugins to the Trash, and the Presets, Themes and `settings.toml`
/// only when asked. Returns true when the app should quit.
pub fn uninstall() -> bool {
    const KEEP: &str = "Uninstall";
    const ALL: &str = "Uninstall and Delete My Presets and Themes";
    let answer = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Uninstall Das-Meter")
        .set_description(
            "This moves Das-Meter and its Send Plugins (CLAP, VST3, AU) to the Trash. \
             Your Presets and Themes stay unless you delete them too.",
        )
        .set_buttons(MessageButtons::YesNoCancelCustom(
            KEEP.into(),
            ALL.into(),
            "Cancel".into(),
        ))
        .show();
    let also_user_data = match answer {
        MessageDialogResult::Custom(choice) if choice == KEEP => false,
        MessageDialogResult::Custom(choice) if choice == ALL => true,
        _ => return false,
    };
    let (Some(app), Some(places), Some(data)) = (
        updates::this_app(),
        Places::for_this_app(),
        crate::preset_files::app_folder(),
    ) else {
        MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Uninstall Das-Meter")
            .set_description("Das-Meter isn't running from its app, so there's nothing to remove.")
            .set_buttons(MessageButtons::Ok)
            .show();
        return false;
    };
    let paths = plugin_install::uninstall_paths(&app, &places, &data, also_user_data);
    let failed: Vec<&PathBuf> = paths.iter().filter(|p| !trash(p)).collect();
    if !failed.is_empty() {
        let list: Vec<String> = failed.iter().map(|p| p.display().to_string()).collect();
        MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Uninstall Das-Meter")
            .set_description(format!(
                "These couldn't be moved to the Trash:\n{}",
                list.join("\n")
            ))
            .set_buttons(MessageButtons::Ok)
            .show();
    }
    true
}

/// Moves `path` to the Trash (so a mistaken uninstall can be undone).
fn trash(path: &Path) -> bool {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .is_ok()
}
