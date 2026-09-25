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
    /// Launch at Login switched on or off.
    LaunchAtLogin(bool),
    /// Bring the app's windows forward (the menu bar icon).
    ShowWindows,
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
    show_in_dock: CheckMenuItem,
    launch_at_login: CheckMenuItem,
    show_welcome: MenuItem,
    /// The menu bar icon and its menu's items.
    _tray: tray_icon::TrayIcon,
    tray_show: MenuItem,
    tray_settings: MenuItem,
    shown_toggles: Option<(bool, bool)>,
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
        let show_in_dock = CheckMenuItem::new("Show in Dock", true, true, None);
        let launch_at_login = CheckMenuItem::new("Launch at Login", true, false, None);
        let show_welcome = MenuItem::new("Show Welcome", true, None);
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
                &show_in_dock,
                &launch_at_login,
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
        let help_menu = Submenu::with_items("Help", true, &[&help, &show_welcome])
            .expect("build the Help menu");
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

        // The menu bar icon: always there, so Das-Meter stays reachable
        // without its Dock icon.
        let tray_show = MenuItem::new("Show Das-Meter", true, None);
        let tray_settings = MenuItem::new("Settings…", true, None);
        let tray_menu = Menu::with_items(&[
            &tray_show,
            &tray_settings,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(Some("Quit Das-Meter")),
        ])
        .expect("build the menu bar icon's menu");
        let tray = tray_icon::TrayIconBuilder::new()
            .with_icon(menu_bar_icon())
            .with_icon_as_template(true)
            .with_tooltip("Das-Meter")
            .with_menu(Box::new(tray_menu))
            .build()
            .expect("add the menu bar icon");

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
            show_in_dock,
            launch_at_login,
            show_welcome,
            _tray: tray,
            tray_show,
            tray_settings,
            shown_toggles: None,
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
                if id == self.settings || id == *self.tray_settings.id() {
                    Some(Command::Core(Event::ShowSettings(true)))
                } else if id == *self.tray_show.id() {
                    Some(Command::ShowWindows)
                } else if id == *self.show_in_dock.id() {
                    // A check item toggles itself before the click arrives.
                    Some(Command::Core(Event::SetShowInDock(
                        self.show_in_dock.is_checked(),
                    )))
                } else if id == *self.launch_at_login.id() {
                    Some(Command::LaunchAtLogin(self.launch_at_login.is_checked()))
                } else if id == *self.show_welcome.id() {
                    Some(Command::Core(Event::ShowWelcome))
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

    /// Ticks Show in Dock and Launch at Login as the app core has them.
    pub fn show_toggles(&mut self, show_in_dock: bool, launch_at_login: bool) {
        let shown = (show_in_dock, launch_at_login);
        if self.shown_toggles == Some(shown) {
            return;
        }
        self.shown_toggles = Some(shown);
        self.show_in_dock.set_checked(show_in_dock);
        self.launch_at_login.set_checked(launch_at_login);
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

/// The menu bar icon: four level bars, drawn as a template image (macOS
/// tints it for the menu bar), 18 points tall at 2x.
fn menu_bar_icon() -> tray_icon::Icon {
    const SIZE: u32 = 36;
    // Each bar's x range and top, in pixels; they all end at the bottom.
    const BARS: [(u32, u32, u32); 4] = [(4, 10, 16), (12, 18, 6), (20, 26, 12), (28, 34, 20)];
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let on = BARS
                .iter()
                .any(|&(left, right, top)| x >= left && x < right && y >= top && y < SIZE - 3);
            if on {
                let i = ((y * SIZE + x) * 4) as usize;
                rgba[i + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("a valid icon")
}
