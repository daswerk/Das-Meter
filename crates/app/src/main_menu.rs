//! The macOS menu bar: the app menu with Settings…, Listen to and Help.
//!
//! A skeleton for now: the Presets, Bar and install tickets add their items.

use std::sync::mpsc;

use dasmeter_core::settings::HELP_URL;
use dasmeter_core::{Event, ListenTo};
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
}

pub struct MainMenu {
    _menu: Menu,
    settings: MenuId,
    system_capture: CheckMenuItem,
    send_plugins: CheckMenuItem,
    help: MenuId,
    clicks: mpsc::Receiver<MenuId>,
    shown: Option<ListenTo>,
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
                &PredefinedMenuItem::separator(),
                &settings,
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
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::close_window(None),
            ],
        )
        .expect("build the Window menu");
        let help_menu = Submenu::with_items("Help", true, &[&help]).expect("build the Help menu");
        let menu =
            Menu::with_items(&[&app, &listen_to, &window, &help_menu]).expect("build the menu bar");
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
                } else if id == self.help {
                    Some(Command::Open(HELP_URL))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Ticks the Listen to item the app core is on. (A click on a check item
    /// toggles it on its own, so this runs after every click too.)
    pub fn show(&mut self, listen_to: ListenTo, force: bool) {
        if self.shown == Some(listen_to) && !force {
            return;
        }
        self.shown = Some(listen_to);
        self.system_capture
            .set_checked(listen_to == ListenTo::SystemCapture);
        self.send_plugins
            .set_checked(listen_to == ListenTo::SendPlugins);
    }
}
