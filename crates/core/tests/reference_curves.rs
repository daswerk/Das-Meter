//! Reference curves on the Spectrum (#101): saving the long-term average,
//! showing a curve, a curve gone missing, and sharing one in a Preset.

use std::time::Duration;

use dasmeter_analysis::LongTerm;
use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{AppCore, Event, MeterSettings, ReferenceCurve, Scene, ThemeFile};

const RATE: u32 = 48_000;
/// The default Bar: Waveform, Spectrum, Stereometer, Loudness Meter.
const SPECTRUM: usize = 1;

struct App {
    core: AppCore,
    now: Duration,
    curve_files: Vec<ThemeFile>,
}

impl App {
    fn launch() -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
            curve_files: Vec::new(),
        };
        app.core.handle(
            Event::PresetFiles {
                files: &[],
                settings: None,
            },
            app.now,
        );
        app.core.handle(Event::StartListening, app.now);
        app.core
            .handle(Event::CaptureStarted { sample_rate: RATE }, app.now);
        app
    }

    fn long_term(&mut self, long_term: LongTerm) {
        let Some(MeterSettings::Spectrum(mut settings)) = self.core.meter_settings(SPECTRUM) else {
            panic!("meter {SPECTRUM} isn't a Spectrum");
        };
        settings.analysis.long_term = long_term;
        self.core.handle(
            Event::SetMeter {
                meter: SPECTRUM,
                settings: MeterSettings::Spectrum(settings),
            },
            self.now,
        );
    }

    /// Plays a tone, drawing as the app would.
    fn play(&mut self, seconds: f64, hz: f64, dbfs: f64) {
        let audio = both(&sine(
            RATE,
            hz,
            dbfs,
            0.0,
            (seconds * f64::from(RATE)) as usize,
        ));
        for block in audio.chunks(1024) {
            self.now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            self.core.handle(Event::Audio(block), self.now);
            self.core.decide(self.now);
        }
    }

    fn flush(&mut self) {
        for write in self.core.take_curve_writes() {
            self.curve_files.retain(|f| f.file_name != write.file_name);
            self.curve_files.push(ThemeFile {
                file_name: write.file_name,
                text: write.contents,
            });
        }
    }

    fn scene(&mut self) -> Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.flush();
        self.core.scene().unwrap().clone()
    }
}

fn reference_of(scene: &Scene, meter: usize) -> Option<dasmeter_core::scene::ReferenceScene> {
    scene
        .windows
        .iter()
        .flat_map(|w| &w.meters)
        .find(|m| m.meter == meter)
        .and_then(|m| m.reference.clone())
}

#[test]
fn saving_the_long_term_average_writes_a_curve_and_shows_it() {
    let mut app = App::launch();
    app.long_term(LongTerm::Seconds(10.0));
    app.play(2.0, 1_000.0, -20.0);
    app.core.handle(
        Event::SaveReferenceCurve {
            meter: SPECTRUM,
            name: "My mix",
        },
        app.now,
    );
    let scene = app.scene();
    assert_eq!(scene.reference_curves, ["My mix"]);
    assert_eq!(app.curve_files.len(), 1);
    let curve = ReferenceCurve::from_toml(&app.curve_files[0].text).unwrap();
    assert_eq!(curve.name, "My mix");
    assert!(curve.seconds > 1.0);
    let shown = reference_of(&scene, SPECTRUM).expect("the Spectrum shows it");
    assert_eq!(shown.name, "My mix");
    assert!(!shown.missing);
    assert!(!shown.levels.is_empty());
    // The Preset remembers it.
    assert_eq!(
        app.core.current_data().meters[SPECTRUM]
            .reference
            .as_deref(),
        Some("My mix")
    );
}

#[test]
fn without_a_long_term_average_nothing_is_saved() {
    let mut app = App::launch();
    app.play(1.0, 1_000.0, -20.0);
    app.core.handle(
        Event::SaveReferenceCurve {
            meter: SPECTRUM,
            name: "Nothing",
        },
        app.now,
    );
    let scene = app.scene();
    assert!(scene.reference_curves.is_empty());
    assert!(app.curve_files.is_empty());
    assert_eq!(reference_of(&scene, SPECTRUM), None);
}

#[test]
fn a_missing_curve_is_kept_by_name_and_shown_as_missing() {
    let mut app = App::launch();
    app.core.handle(
        Event::ShowReferenceCurve {
            meter: SPECTRUM,
            name: Some("Gone"),
        },
        app.now,
    );
    app.play(0.5, 1_000.0, -20.0);
    let shown = reference_of(&app.scene(), SPECTRUM).unwrap();
    assert!(shown.missing);
    assert!(shown.levels.is_empty());
    // None shows none.
    app.core.handle(
        Event::ShowReferenceCurve {
            meter: SPECTRUM,
            name: None,
        },
        app.now,
    );
    assert_eq!(reference_of(&app.scene(), SPECTRUM), None);
}

#[test]
fn an_exported_preset_carries_its_curve_to_another_install() {
    let mut app = App::launch();
    app.long_term(LongTerm::Whole);
    app.play(2.0, 200.0, -12.0);
    app.core.handle(
        Event::SaveReferenceCurve {
            meter: SPECTRUM,
            name: "Reference",
        },
        app.now,
    );
    app.scene();
    let (_, file) = app.core.export().expect("a Preset is open");
    assert!(file.contains("[[curves]]"));

    let mut other = App::launch();
    other.core.handle(Event::ImportPreset(&file), other.now);
    other.play(0.5, 1_000.0, -20.0);
    let scene = other.scene();
    assert_eq!(scene.reference_curves, ["Reference"]);
    assert_eq!(other.curve_files.len(), 1);
    let shown = reference_of(&scene, SPECTRUM).expect("the imported Preset shows it");
    assert_eq!(shown.name, "Reference");
    assert!(!shown.missing);
}
