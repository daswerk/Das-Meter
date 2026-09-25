//! The macOS menu bar: the app menu with Settings…, Listen to and Help.
//!
//! Also Check for Updates, Update and Uninstall (macOS packaging).

use std::sync::mpsc;

use dasmeter_core::settings::HELP_URL;
use dasmeter_core::{Event, LayoutMode, ListenTo, PresetScene};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{
    AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

/// What a menu bar item asks for.
pub enum Command {
    /// An app core event.
    Core(Event<'static>),
    /// Open this page in the browser.
    Open(&'static str),
    /// Export the current Preset to a file the user picks.
    Export,
    /// Import a Preset file the user picks.
    Import,
    /// Copy the Send Plugin into the per-user plug-in folders.
    InstallSendPlugin,
    /// Look for a new version now.
    CheckForUpdates,
    /// Install the new version the check found.
    Update,
    /// Remove the app and its Send Plugins, after asking.
    Uninstall,
}

pub struct MainMenu {
    _menu: Menu,
    settings: MenuId,
    system_capture: CheckMenuItem,
    send_plugins: CheckMenuItem,
    help: MenuId,
    float_on_top: CheckMenuItem,
    keep_here: MenuItem,
    bar_mode: CheckMenuItem,
    window_mode: CheckMenuItem,
    presets: Submenu,
    preset_items: Vec<CheckMenuItem>,
    revert: MenuItem,
    save_as_new: MenuItem,
    export: MenuItem,
    import: MenuItem,
    shown_presets: Option<PresetScene>,
    install_send_plugin: MenuItem,
    check_for_updates: MenuItem,
    update: MenuItem,
    shown_update: Option<String>,
    uninstall: MenuItem,
    clicks: mpsc::Receiver<MenuId>,
    shown: Option<(ListenTo, LayoutMode, bool)>,
}

impl MainMenu {
    /// Installs the menu bar. Call once the app has finished launching (winit
    /// sets its own menu then). `wake` wakes the event loop on a click.
    pub fn install(wake: impl Fn() + Send + Sync + 'static) -> MainMenu {
        let settings = MenuItem::new(
            "Settings…",
            true,
            Some(Accelerator::new(Modifiers::META, Code::Comma)),
        );
        let system_capture = CheckMenuItem::new("System Capture", true, true, None);
        let send_plugins = CheckMenuItem::new("Send Plugins", true, false, None);
        let help = MenuItem::new("Das-Meter Help", true, None);
        let float_on_top = CheckMenuItem::new("Float on Top", true, true, None);
        let keep_here = MenuItem::new("Keep Windows Here", false, None);
        let bar_mode = CheckMenuItem::new("Bar Mode", true, true, None);
        let window_mode = CheckMenuItem::new("Window Mode", true, false, None);
        let install_send_plugin = MenuItem::new("Install Send Plugin…", true, None);
        let check_for_updates = MenuItem::new("Check for Updates…", true, None);
        // Enabled, and named for the version, once a check finds one.
        let update = MenuItem::new("Update Das-Meter…", false, None);
        let uninstall = MenuItem::new("Uninstall Das-Meter…", true, None);
        let about = AboutMetadata {
            name: Some("Das-Meter".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        };
        let app = Submenu::with_items(
            "Das-Meter",
            true,
            &[
                &PredefinedMenuItem::about(None, Some(about)),
                &check_for_updates,
                &update,
                &PredefinedMenuItem::separator(),
                &settings,
                &install_send_plugin,
                &PredefinedMenuItem::separator(),
                &uninstall,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        )
        .expect("build the app menu");
        let listen_to = Submenu::with_items("Listen to", true, &[&system_capture, &send_plugins])
            .expect("build the Listen to menu");
        let window = Submenu::with_items(
            "Window",
            true,
            &[
                &bar_mode,
                &window_mode,
                &PredefinedMenuItem::separator(),
                &float_on_top,
                &keep_here,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::close_window(None),
            ],
        )
        .expect("build the Window menu");
        let help_menu = Submenu::with_items("Help", true, &[&help]).expect("build the Help menu");
        // Filled from the Preset list; see `show_presets`.
        let presets = Submenu::new("Presets", true);
        let revert = MenuItem::new("Revert Preset", false, None);
        let save_as_new = MenuItem::new("Save as New Preset", true, None);
        let export = MenuItem::new("Export Preset…", true, None);
        let import = MenuItem::new(
            "Import Preset…",
            true,
            Some(Accelerator::new(Modifiers::META, Code::KeyO)),
        );
        let menu = Menu::with_items(&[&app, &listen_to, &presets, &window, &help_menu])
            .expect("build the menu bar");
        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        help_menu.set_as_help_menu_for_nsapp();

        let (sender, clicks) = mpsc::channel();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = sender.send(event.id);
            wake();
        }));
        MainMenu {
            _menu: menu,
            settings: settings.id().clone(),
            system_capture,
            send_plugins,
            help: help.id().clone(),
            float_on_top,
            keep_here,
            presets,
            preset_items: Vec::new(),
            revert,
            save_as_new,
            export,
            import,
            shown_presets: None,
            install_send_plugin,
            check_for_updates,
            update,
            shown_update: None,
            uninstall,
            bar_mode,
            window_mode,
            clicks,
            shown: None,
        }
    }

    /// The items clicked since the last call, as commands.
    pub fn commands(&self) -> Vec<Command> {
        self.clicks
            .try_iter()
            .filter_map(|id| {
                if id == self.settings {
                    Some(Command::Core(Event::ShowSettings(true)))
                } else if id == *self.system_capture.id() {
                    Some(Command::Core(Event::SetListenTo(ListenTo::SystemCapture)))
                } else if id == *self.send_plugins.id() {
                    Some(Command::Core(Event::SetListenTo(ListenTo::SendPlugins)))
                } else if id == *self.float_on_top.id() {
                    // The Bar's screen button (on macOS only Float on top and
                    // Normal window), or the Window's Always on top.
                    Some(Command::Core(Event::ToggleOnTop))
                } else if id == *self.keep_here.id() {
                    Some(Command::Core(Event::KeepHere))
                } else if id == *self.bar_mode.id() {
                    Some(Command::Core(Event::SetMode(LayoutMode::Bar)))
                } else if id == *self.window_mode.id() {
                    Some(Command::Core(Event::SetMode(LayoutMode::Window)))
                } else if id == *self.revert.id() {
                    Some(Command::Core(Event::RevertPreset))
                } else if id == *self.save_as_new.id() {
                    Some(Command::Core(Event::SavePresetAsNew))
                } else if id == *self.export.id() {
                    Some(Command::Export)
                } else if id == *self.import.id() {
                    Some(Command::Import)
                } else if let Some(index) = self.preset_items.iter().position(|i| *i.id() == id) {
                    Some(Command::Core(Event::SwitchPreset { index }))
                } else if id == *self.install_send_plugin.id() {
                    Some(Command::InstallSendPlugin)
                } else if id == *self.check_for_updates.id() {
                    Some(Command::CheckForUpdates)
                } else if id == *self.update.id() {
                    Some(Command::Update)
                } else if id == *self.uninstall.id() {
                    Some(Command::Uninstall)
                } else if id == self.help {
                    Some(Command::Open(HELP_URL))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Offers the update the check found (or none).
    pub fn show_update(&mut self, version: Option<&str>) {
        if self.shown_update.as_deref() == version {
            return;
        }
        self.shown_update = version.map(str::to_string);
        match version {
            Some(version) => {
                self.update
                    .set_text(format!("Update to Das-Meter {version}…"));
                self.update.set_enabled(true);
            }
            None => {
                self.update.set_text("Update Das-Meter…");
                self.update.set_enabled(false);
            }
        }
    }

    /// Offers Keep Windows Here while a window's display is missing.
    pub fn show_missing_displays(&mut self, missing: &[String]) {
        self.keep_here.set_enabled(!missing.is_empty());
    }

    /// Lists the Presets (⌘1–9 for the first nine), ticks the current one, and
    /// offers Revert when there's something to revert.
    pub fn show_presets(&mut self, presets: &PresetScene, force: bool) {
        if self.shown_presets.as_ref() == Some(presets) && !force {
            return;
        }
        let same_list = self
            .shown_presets
            .as_ref()
            .is_some_and(|shown| shown.list == presets.list);
        self.shown_presets = Some(presets.clone());
        if !same_list {
            while self.presets.remove_at(0).is_some() {}
            const DIGITS: [Code; 9] = [
                Code::Digit1,
                Code::Digit2,
                Code::Digit3,
                Code::Digit4,
                Code::Digit5,
                Code::Digit6,
                Code::Digit7,
                Code::Digit8,
                Code::Digit9,
            ];
            self.preset_items = presets
                .list
                .iter()
                .enumerate()
                .map(|(i, preset)| {
                    let shortcut = DIGITS
                        .get(i)
                        .map(|&code| Accelerator::new(Modifiers::META, code));
                    CheckMenuItem::new(&preset.name, !preset.broken, false, shortcut)
                })
                .collect();
            for item in &self.preset_items {
                let _ = self.presets.append(item);
            }
            let _ = self.presets.append(&PredefinedMenuItem::separator());
            let _ = self.presets.append(&self.revert);
            let _ = self.presets.append(&self.save_as_new);
            let _ = self.presets.append(&PredefinedMenuItem::separator());
            let _ = self.presets.append(&self.import);
            let _ = self.presets.append(&self.export);
        }
        for (i, item) in self.preset_items.iter().enumerate() {
            item.set_checked(presets.current == Some(i));
        }
        self.revert.set_enabled(presets.can_revert);
    }

    /// Ticks the Listen to item the app core is on, and Float on Top when the
    /// Bar floats. (A click on a check item toggles it on its own, so this runs
    /// after every click too.)
    pub fn show(&mut self, listen_to: ListenTo, mode: LayoutMode, float_on_top: bool, force: bool) {
        let shown = (listen_to, mode, float_on_top);
        if self.shown == Some(shown) && !force {
            return;
        }
        self.shown = Some(shown);
        self.float_on_top.set_checked(float_on_top);
        self.bar_mode.set_checked(mode == LayoutMode::Bar);
        self.window_mode.set_checked(mode == LayoutMode::Window);
        self.system_capture
            .set_checked(listen_to == ListenTo::SystemCapture);
        self.send_plugins
            .set_checked(listen_to == ListenTo::SendPlugins);
    }
}
