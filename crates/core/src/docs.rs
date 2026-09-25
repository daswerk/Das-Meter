//! The docs site's pages and anchors, in one place: the app's Help and
//! "Learn more" links point here, and the docs keep these anchors stable.

/// Where the docs site is published (GitHub Pages, built from `docs/site`).
pub const SITE: &str = "https://daswerk.github.io/Das-Meter/";

/// A page, or a section of one, the app links to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    /// The docs' front page.
    Home,
    /// Download, install and the macOS Open Anyway steps.
    GettingStarted,
    OpenAnyway,
    /// Listen to: System Capture and Send Plugins.
    ListenTo,
    /// The macOS permission for System Capture.
    MacPrivacy,
    /// Hearing nothing: ASIO and other exclusive-mode drivers.
    Asio,
    /// Send Plugins: install, naming, Waiting for.
    SendPlugins,
    InstallSendPlugin,
    /// The four Meters and their settings.
    Meters,
    /// Bar, Pop-outs, Window mode, Reserve space, fullscreen.
    Layout,
    VirtualDesktops,
    MultipleDisplays,
    Themes,
    Presets,
    Sharing,
    Updates,
    Uninstall,
    /// What the readings mean and the standards they follow.
    Measurements,
}

impl Topic {
    pub const ALL: [Topic; 18] = [
        Topic::Home,
        Topic::GettingStarted,
        Topic::OpenAnyway,
        Topic::ListenTo,
        Topic::MacPrivacy,
        Topic::Asio,
        Topic::SendPlugins,
        Topic::InstallSendPlugin,
        Topic::Meters,
        Topic::Layout,
        Topic::VirtualDesktops,
        Topic::MultipleDisplays,
        Topic::Themes,
        Topic::Presets,
        Topic::Sharing,
        Topic::Updates,
        Topic::Uninstall,
        Topic::Measurements,
    ];

    /// The page and anchor under [`SITE`].
    pub fn path(self) -> &'static str {
        match self {
            Topic::Home => "",
            Topic::GettingStarted => "getting-started.html",
            Topic::OpenAnyway => "getting-started.html#open-anyway",
            Topic::ListenTo => "listen-to.html",
            Topic::MacPrivacy => "listen-to.html#macos-permission",
            Topic::Asio => "listen-to.html#hearing-nothing",
            Topic::SendPlugins => "send-plugins.html",
            Topic::InstallSendPlugin => "send-plugins.html#install",
            Topic::Meters => "meters.html",
            Topic::Layout => "layout.html",
            Topic::VirtualDesktops => "layout.html#virtual-desktops",
            Topic::MultipleDisplays => "layout.html#multiple-displays",
            Topic::Themes => "themes.html",
            Topic::Presets => "presets.html",
            Topic::Sharing => "presets.html#sharing",
            Topic::Updates => "updates.html",
            Topic::Uninstall => "updates.html#uninstall",
            Topic::Measurements => "measurements.html",
        }
    }

    /// The full link.
    pub fn url(self) -> String {
        format!("{SITE}{}", self.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_has_its_own_link() {
        let mut urls: Vec<String> = Topic::ALL.iter().map(|t| t.url()).collect();
        urls.sort();
        urls.dedup();
        assert_eq!(urls.len(), Topic::ALL.len());
        assert!(urls.iter().all(|u| u.starts_with(SITE)));
    }
}
