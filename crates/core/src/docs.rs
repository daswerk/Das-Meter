//! The docs site's pages and anchors, in one place: the app's Help and
//! "Learn more" links point here, and the docs keep these anchors stable.

/// Where the docs site is published (GitHub Pages, built from `docs/site`).
/// A test checks every [`Topic`] against the pages' headings.
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
    /// The five Meters and their settings.
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

    /// mdBook's heading ids: lower case, spaces to dashes, other punctuation dropped.
    fn slug(heading: &str) -> String {
        heading
            .trim()
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                ' ' => Some('-'),
                c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_topic_is_a_page_and_heading_of_the_docs_site() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/site/src");
        for topic in Topic::ALL {
            let (page, anchor) = topic.path().split_once('#').unwrap_or((topic.path(), ""));
            let file = match page {
                "" => "README.md".to_owned(),
                page => page.replace(".html", ".md"),
            };
            let text = std::fs::read_to_string(src.join(&file))
                .unwrap_or_else(|_| panic!("{topic:?}: docs/site/src/{file} is missing"));
            if !anchor.is_empty() {
                let found = text
                    .lines()
                    .filter_map(|line| line.strip_prefix("## "))
                    .any(|heading| slug(heading) == anchor);
                assert!(found, "{topic:?}: no heading for #{anchor} in {file}");
            }
        }
    }

    #[test]
    fn the_loudness_settings_link_to_the_measurements_page() {
        assert_eq!(crate::settings::MEASUREMENTS_URL, Topic::Measurements.url());
    }

    #[test]
    fn every_topic_has_its_own_link() {
        let mut urls: Vec<String> = Topic::ALL.iter().map(|t| t.url()).collect();
        urls.sort();
        urls.dedup();
        assert_eq!(urls.len(), Topic::ALL.len());
        assert!(urls.iter().all(|u| u.starts_with(SITE)));
    }
}
