//! The Tonal Balance Meter in the app core (#105): teaching a Target by
//! playing, adding songs from files, the key suggestion and shift, Freeze,
//! and sharing a Target in a Preset.

use std::time::Duration;

use dasmeter_analysis::LongTerm;
use dasmeter_analysis::signals::{both, silence, sine};
use dasmeter_analysis::tonal_balance::{Key, Parts, tonal_grid};
use dasmeter_core::{
    AppCore, Event, KeyChoice, MeterScene, MeterSettings, MeterState, MeterView, Scene, Target,
    TargetScene, TargetSong, ThemeFile, TonalBalanceMeterSettings,
};

const RATE: u32 = 48_000;
const METER: usize = 0;
const A_MINOR: Key = Key {
    tonic: 9,
    minor: true,
};
const F_MINOR: Key = Key {
    tonic: 5,
    minor: true,
};

struct App {
    core: AppCore,
    now: Duration,
    target_files: Vec<ThemeFile>,
}

impl App {
    fn launch() -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
            target_files: Vec::new(),
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
        app.set(TonalBalanceMeterSettings::default());
        app
    }

    fn set(&mut self, settings: TonalBalanceMeterSettings) {
        self.core.handle(
            Event::SetMeter {
                meter: METER,
                settings: MeterSettings::TonalBalance(settings),
            },
            self.now,
        );
    }

    fn settings(&self) -> TonalBalanceMeterSettings {
        match self.core.meter_settings(METER) {
            Some(MeterSettings::TonalBalance(s)) => s,
            other => panic!("not a Tonal Balance Meter: {other:?}"),
        }
    }

    fn feed(&mut self, audio: &[f32]) {
        for block in audio.chunks(2048) {
            self.now += Duration::from_secs_f64(1024.0 / f64::from(RATE));
            self.core.handle(Event::Audio(block), self.now);
            self.core.decide(self.now);
        }
    }

    /// A bass note with a few harmonics, Left and Right alike.
    fn play(&mut self, seconds: f64, hz: f64) {
        let frames = (seconds * f64::from(RATE)) as usize;
        let mut mono = vec![0.0f32; frames];
        for (harmonic, dbfs) in [(1.0, -12.0), (2.0, -18.0), (3.0, -24.0), (8.0, -30.0)] {
            for (sum, s) in mono
                .iter_mut()
                .zip(sine(RATE, hz * harmonic, dbfs, 0.0, frames))
            {
                *sum += s;
            }
        }
        self.feed(&both(&mono));
    }

    fn flush(&mut self) {
        for write in self.core.take_target_writes() {
            self.target_files.retain(|f| f.file_name != write.file_name);
            self.target_files.push(ThemeFile {
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

    fn meter(&mut self) -> MeterScene {
        self.scene()
            .windows
            .iter()
            .flat_map(|w| &w.meters)
            .find(|m| m.meter == METER)
            .cloned()
            .expect("the Meter is shown")
    }

    fn target(&mut self) -> Option<TargetScene> {
        self.meter()
            .tonal_balance
            .expect("a Tonal Balance Meter")
            .target
    }

    /// The view's mix and frozen ghost, and the seconds learned.
    fn view(&mut self) -> (bool, Option<Vec<f32>>, Option<u32>) {
        let meter = self.meter();
        let state = meter.tonal_balance.expect("a Tonal Balance Meter");
        match meter.state {
            MeterState::Live(MeterView::TonalBalance { mix, frozen, .. }) => {
                assert_eq!(frozen.is_some(), state.frozen);
                (mix.is_some(), frozen, state.learning)
            }
            other => panic!("not a live Tonal Balance Meter: {other:?}"),
        }
    }
}

/// A song as files would give it: flat but for a peak at `peak_hz`.
fn song(name: &str, key: Key, peak_hz: f32) -> TargetSong {
    let grid = tonal_grid();
    let peak = grid
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1 / peak_hz)
                .log2()
                .abs()
                .total_cmp(&(b.1 / peak_hz).log2().abs())
        })
        .unwrap()
        .0;
    let mut curve = vec![-30.0; grid.len()];
    curve[peak] = -10.0;
    TargetSong {
        name: name.into(),
        seconds: 200.0,
        loudness: -8.0,
        parts: Parts::Loud,
        key: Some(key),
        key_confidence: 0.6,
        sub_crest: Some(8.0),
        bass_crest: Some(10.0),
        low_end_mono: Some(0.98),
        regions: [-12.0, -10.0, -14.0, -18.0, -24.0],
        curve,
    }
}

#[test]
fn learning_by_playing_adds_a_song_and_compares_with_it() {
    let mut app = App::launch();
    app.core.handle(Event::LearnSong { meter: METER }, app.now);
    app.play(6.0, 55.0);
    let (heard, _, learning) = app.view();
    assert!(heard, "the mix's average is shown while learning");
    assert!(learning.unwrap() >= 5, "learning counts what it heard");

    app.core.handle(
        Event::AddLearnedSong {
            meter: METER,
            target: "Techno",
            song: "",
        },
        app.now,
    );
    let scene = app.scene();
    assert_eq!(scene.targets, ["Techno"]);
    assert_eq!(app.target_files.len(), 1);
    let file = Target::from_toml(&app.target_files[0].text).unwrap();
    assert_eq!(file.songs.len(), 1);
    assert_eq!(file.songs[0].name, "Song 1");

    let target = app.target().expect("the Meter compares with it");
    assert_eq!(target.name, "Techno");
    assert!(!target.missing);
    assert_eq!(target.curve.len(), tonal_grid().len());
    assert!(target.regions.is_some());
    assert_eq!(app.view().2, None, "learning ended");
    // The Preset remembers it.
    assert_eq!(
        app.core.current_data().meters[METER].reference.as_deref(),
        Some("Techno")
    );
}

#[test]
fn silence_teaches_nothing() {
    let mut app = App::launch();
    app.core.handle(Event::LearnSong { meter: METER }, app.now);
    app.feed(&both(&silence(RATE as usize * 3)));
    app.core.handle(
        Event::AddLearnedSong {
            meter: METER,
            target: "Quiet",
            song: "Nothing",
        },
        app.now,
    );
    assert!(app.scene().targets.is_empty());
    assert!(app.target_files.is_empty());
    // Still learning; Cancel stops it.
    assert!(app.view().2.is_some());
    app.core
        .handle(Event::CancelLearning { meter: METER }, app.now);
    assert_eq!(app.view().2, None);
}

#[test]
fn the_target_is_drawn_in_its_songs_key_and_shifting_moves_it_to_the_mix() {
    let mut app = App::launch();
    let songs = [song("One", A_MINOR, 55.0), song("Two", A_MINOR, 55.0)];
    app.core.handle(
        Event::AddTargetSongs {
            meter: METER,
            target: "Club",
            songs: &songs,
        },
        app.now,
    );
    app.play(1.0, 110.0);
    let at_home = app.target().unwrap();
    assert_eq!(at_home.songs.len(), 2);
    assert_eq!(at_home.home_key, Some(A_MINOR));
    assert_eq!(at_home.key, Some(A_MINOR));
    assert_eq!(at_home.suggest_shift, None, "the mix's key isn't sure");

    let fixed = TonalBalanceMeterSettings {
        key: KeyChoice::Fixed(F_MINOR),
        ..app.settings()
    };
    app.set(fixed);
    let suggesting = app.target().unwrap();
    assert_eq!(suggesting.suggest_shift, Some(F_MINOR));
    assert_eq!(suggesting.curve, at_home.curve, "not moved until asked");

    app.set(TonalBalanceMeterSettings {
        shift_to_key: true,
        ..fixed
    });
    let shifted = app.target().unwrap();
    assert_eq!(shifted.key, Some(F_MINOR));
    assert_eq!(shifted.suggest_shift, None);
    let loudest = |t: &TargetScene| {
        let grid = tonal_grid();
        let (i, _) = t
            .curve
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.average.total_cmp(&b.1.average))
            .unwrap();
        grid[i]
    };
    // A (55 Hz) down four semitones is F (43.7 Hz).
    assert!((loudest(&at_home) / 55.0).log2().abs() < 1.0 / 48.0);
    assert!((loudest(&shifted) / 43.65).log2().abs() < 1.0 / 24.0);
}

#[test]
fn removing_a_song_recomputes_the_target() {
    let mut app = App::launch();
    let mut quiet = song("Quiet", A_MINOR, 55.0);
    quiet.regions = [-20.0; 5];
    let songs = [song("Loud", A_MINOR, 55.0), quiet];
    app.core.handle(
        Event::AddTargetSongs {
            meter: METER,
            target: "Mixed",
            songs: &songs,
        },
        app.now,
    );
    app.play(0.6, 110.0);
    assert_eq!(app.target().unwrap().regions.unwrap()[0].average, -16.0);
    app.core.handle(
        Event::RemoveTargetSong {
            target: "Mixed",
            song: 1,
        },
        app.now,
    );
    let target = app.target().unwrap();
    assert_eq!(target.songs.len(), 1);
    assert_eq!(target.regions.unwrap()[0].average, -12.0);
    let file = Target::from_toml(&app.target_files[0].text).unwrap();
    assert_eq!(file.songs.len(), 1);
}

#[test]
fn freeze_keeps_a_ghost_of_the_curve() {
    let mut app = App::launch();
    app.core
        .handle(Event::FreezeTonalBalance { meter: METER }, app.now);
    assert_eq!(app.view().1, None, "nothing heard, nothing to freeze");
    app.play(2.0, 55.0);
    app.core
        .handle(Event::FreezeTonalBalance { meter: METER }, app.now);
    let ghost = app.view().1.expect("frozen");
    assert_eq!(ghost.len(), tonal_grid().len());
    app.play(2.0, 220.0);
    assert_eq!(app.view().1, Some(ghost), "the ghost stays put");
    app.core
        .handle(Event::FreezeTonalBalance { meter: METER }, app.now);
    assert_eq!(app.view().1, None);
}

#[test]
fn a_missing_target_is_kept_by_name() {
    let mut app = App::launch();
    app.core.handle(
        Event::ChooseTarget {
            meter: METER,
            name: Some("Gone"),
        },
        app.now,
    );
    let target = app.target().unwrap();
    assert!(target.missing);
    assert!(target.curve.is_empty());
    app.core.handle(
        Event::ChooseTarget {
            meter: METER,
            name: None,
        },
        app.now,
    );
    assert_eq!(app.target(), None);
}

#[test]
fn the_average_is_never_off_and_the_cutoff_stays_in_range() {
    let mut app = App::launch();
    app.set(TonalBalanceMeterSettings {
        averaging: LongTerm::Off,
        mono_cutoff: 5_000.0,
        ..TonalBalanceMeterSettings::default()
    });
    let settings = app.settings();
    assert_eq!(settings.averaging, LongTerm::Seconds(10.0));
    assert_eq!(settings.mono_cutoff, 300.0);
}

#[test]
fn an_exported_preset_carries_its_target_to_another_install() {
    let mut app = App::launch();
    let songs = [song("One", A_MINOR, 55.0)];
    app.core.handle(
        Event::AddTargetSongs {
            meter: METER,
            target: "Shared",
            songs: &songs,
        },
        app.now,
    );
    app.scene();
    let (_, file) = app.core.export().expect("a Preset is open");
    assert!(file.contains("[[targets]]"));
    assert!(!file.contains("[[curves]]"));

    let mut other = App::launch();
    other.core.handle(Event::ImportPreset(&file), other.now);
    other.play(0.6, 110.0);
    let scene = other.scene();
    assert_eq!(scene.targets, ["Shared"]);
    assert_eq!(other.target_files.len(), 1);
    let target = other
        .target()
        .expect("the imported Preset compares with it");
    assert_eq!(target.name, "Shared");
    assert_eq!(target.songs.len(), 1);
}

#[test]
fn a_compared_file_takes_the_live_sources_place() {
    let mut app = App::launch();
    app.play(2.0, 220.0);
    let summary = dasmeter_analysis::tonal_balance::Summary {
        curve: vec![-20.0; tonal_grid().len()],
        regions: [-20.0; 5],
        loudness: -10.0,
        low_end_mono: Some(0.5),
        low_end_crest: [None; 2],
        sub: None,
        bass: None,
        key: None,
        seconds: 180.0,
        chroma: [0.0; 12],
    };
    app.core.handle(
        Event::CompareFile {
            meter: METER,
            file: Some(("Bounce", &summary)),
        },
        app.now,
    );
    let meter = app.meter();
    assert_eq!(
        meter.tonal_balance.unwrap().comparing.as_deref(),
        Some("Bounce")
    );
    let MeterState::Live(MeterView::TonalBalance { mix: Some(mix), .. }) = meter.state else {
        panic!("no mix");
    };
    assert_eq!(
        mix.low_end_mono,
        Some(0.5),
        "the file's figures, not the live ones"
    );
    app.core.handle(
        Event::CompareFile {
            meter: METER,
            file: None,
        },
        app.now,
    );
    assert_eq!(app.meter().tonal_balance.unwrap().comparing, None);
}

#[test]
fn upload_progress_shows_on_the_meter_until_it_is_over() {
    let mut app = App::launch();
    let progress = dasmeter_core::UploadScene {
        done: 1,
        total: 3,
        failed: vec![("Broken".into(), "not an audio file".into())],
    };
    app.core.handle(
        Event::UploadProgress {
            meter: METER,
            progress: Some(&progress),
        },
        app.now,
    );
    assert_eq!(
        app.meter().tonal_balance.unwrap().upload,
        Some(progress.clone())
    );
    app.core.handle(
        Event::UploadProgress {
            meter: METER,
            progress: None,
        },
        app.now,
    );
    assert_eq!(app.meter().tonal_balance.unwrap().upload, None);
}

#[test]
fn the_drawn_curve_glides_to_a_new_frame_instead_of_jumping() {
    let mut app = App::launch();
    app.set(TonalBalanceMeterSettings {
        averaging: LongTerm::Seconds(3.0),
        ..TonalBalanceMeterSettings::default()
    });
    let curve = |app: &mut App| match app.meter().state {
        MeterState::Live(MeterView::TonalBalance { mix: Some(mix), .. }) => mix.curve.clone(),
        other => panic!("no mix yet: {other:?}"),
    };
    app.play(4.0, 55.0);
    let before = curve(&mut app);
    // Another note: its frame arrives within half a second, and the curve
    // is only part of the way there just after.
    app.play(0.55, 82.4);
    let soon = curve(&mut app);
    app.feed(&silence(4 * RATE as usize));
    let settled = curve(&mut app);
    let moved = |a: &[f32], b: &[f32]| {
        a.iter()
            .zip(b)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max)
    };
    let (first, rest) = (moved(&before, &soon), moved(&soon, &settled));
    assert!(first > 0.0, "the curve moved toward the new note");
    assert!(rest > 0.0, "it was still on its way: {first} then {rest}");
    // Silence adds no frames, so the curve settles where the average is.
    let later = {
        app.feed(&silence(4 * RATE as usize));
        curve(&mut app)
    };
    assert_eq!(later, settled, "settled curves stop moving");
}
