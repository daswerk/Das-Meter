//! Listen to and picking a Send Plugin per Meter, run headless: Send Plugin
//! lists and audio in, the scene and the listened-to set out.

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, sine};
use dasmeter_core::{
    AppCore, Colour, Decision, Event, Level, ListenTo, LoudnessMeterSettings, MeterScene,
    MeterSettings, MeterState, MeterView, Note, SendPlugin, SendPluginState, SourceLabel,
    SpectrumMeterSettings, WindowKey,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 512;
/// How often the shell lists the Send Plugins.
const POLL: Duration = Duration::from_millis(250);

/// Two Meters: a Spectrum (0) and a Loudness Meter (1).
struct Harness {
    core: AppCore,
    now: Duration,
    listed: Vec<SendPlugin>,
}

fn plugin(id: u64, name: &str) -> SendPlugin {
    SendPlugin {
        id,
        name: name.to_owned(),
        colour: 0x20_80_ff,
        mono: false,
        sample_rate: RATE,
        state: SendPluginState::Live,
        outdated: false,
    }
}

impl Harness {
    fn new() -> Harness {
        let mut core = AppCore::with_meters(vec![
            MeterSettings::Spectrum(SpectrumMeterSettings::default()),
            MeterSettings::Loudness(LoudnessMeterSettings::default()),
        ]);
        // Past the welcome card: System Capture may run.
        core.handle(Event::StartListening, Duration::ZERO);
        Harness {
            core,
            now: Duration::ZERO,
            listed: Vec::new(),
        }
    }

    /// Listening to Send Plugins, with these listed.
    fn on_send_plugins(listed: Vec<SendPlugin>) -> Harness {
        let mut app = Harness::new();
        app.send(Event::SetListenTo(ListenTo::SendPlugins));
        app.list(listed);
        app
    }

    fn send(&mut self, event: Event) -> &mut Harness {
        self.core.handle(event, self.now);
        self
    }

    /// The transport now lists these Send Plugins.
    fn list(&mut self, listed: Vec<SendPlugin>) {
        self.listed = listed;
        let listed = self.listed.clone();
        self.send(Event::SendPlugins(&listed));
    }

    /// Changes one listed Send Plugin and lists again.
    fn change(&mut self, id: u64, change: impl FnOnce(&mut SendPlugin)) {
        let mut listed = self.listed.clone();
        change(listed.iter_mut().find(|p| p.id == id).expect("listed"));
        self.list(listed);
    }

    /// Plays `audio` from the listened-to Send Plugins in real-time blocks,
    /// listing them every poll as the shell does. `sources` maps an ID to the
    /// audio it sends.
    fn play(&mut self, seconds: f64, sources: &[(u64, &[f32])]) {
        let block_time = Duration::from_secs_f64(BLOCK as f64 / f64::from(RATE));
        let blocks = (seconds / block_time.as_secs_f64()).round() as usize;
        let mut next_poll = self.now + POLL;
        for block in 0..blocks {
            self.now += block_time;
            for &(id, audio) in sources {
                if !self.core.listened().contains(&id) {
                    continue;
                }
                let start = (block * BLOCK * 2) % audio.len();
                let end = (start + BLOCK * 2).min(audio.len());
                self.send(Event::SendPluginAudio {
                    id,
                    frames: &audio[start..end],
                });
            }
            if self.now >= next_poll {
                let listed = self.listed.clone();
                self.send(Event::SendPlugins(&listed));
                next_poll += POLL;
            }
        }
    }

    /// Lets `seconds` pass with no audio, listing every poll.
    fn wait(&mut self, seconds: f64) {
        self.play(seconds, &[]);
    }

    fn meters(&mut self) -> Vec<MeterScene> {
        // Leave the frame-rate cap behind, then draw.
        self.now += Duration::from_millis(20);
        self.core.decide(self.now);
        let scene = self.core.scene().expect("a scene was drawn");
        scene.windows[0].meters.clone()
    }

    fn states(&mut self) -> Vec<MeterState> {
        self.meters().into_iter().map(|m| m.state).collect()
    }

    fn notes(&self) -> Vec<Note> {
        self.core.scene().expect("a scene").notes.clone()
    }

    /// The Loudness Meter's momentary loudness (Meter 1).
    fn momentary(&mut self) -> Level {
        match &self.states()[1] {
            MeterState::Live(MeterView::Loudness { display, .. }) => display.momentary,
            other => panic!("expected a live Loudness Meter, got {other:?}"),
        }
    }

    fn sample_rate(&mut self) -> u32 {
        match &self.states()[1] {
            MeterState::Live(MeterView::Loudness { display, .. }) => display.sample_rate,
            other => panic!("expected a live Loudness Meter, got {other:?}"),
        }
    }
}

fn tone(dbfs: f64, seconds: f64) -> Vec<f32> {
    both(&sine(RATE, 1_000.0, dbfs, 0.0, frames(RATE, seconds)))
}

#[track_caller]
fn assert_near(level: Level, want: f64) {
    let got = level.db().unwrap_or_else(|| panic!("silent, want {want}"));
    assert!((got - want).abs() <= 0.5, "got {got}, want {want}");
}

fn is_live(state: &MeterState) -> bool {
    matches!(state, MeterState::Live(_))
}

#[test]
fn listen_to_starts_on_system_capture() {
    let mut app = Harness::new();
    assert_eq!(app.core.listen_to(), ListenTo::SystemCapture);
    app.list(vec![plugin(1, "Kick")]);
    assert!(
        app.core.listened().is_empty(),
        "Send Plugins aren't listened to"
    );
    assert!(app.states().iter().all(|s| *s == MeterState::Starting));
}

#[test]
fn with_no_send_plugins_every_meter_says_how_to_add_one() {
    let mut app = Harness::on_send_plugins(vec![]);
    assert!(app.states().iter().all(|s| *s == MeterState::NoSendPlugins));
    assert_eq!(MeterState::NO_SEND_PLUGINS, "No Send Plugins yet");
    assert_eq!(
        MeterState::NO_SEND_PLUGINS_HINT,
        "Add Das-Meter Send to a track"
    );

    // A gone one doesn't count.
    let mut gone = plugin(1, "Kick");
    gone.state = SendPluginState::Gone;
    app.list(vec![gone]);
    assert!(app.states().iter().all(|s| *s == MeterState::NoSendPlugins));
}

#[test]
fn the_only_send_plugin_is_taken_by_every_meter() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Master")]);
    assert_eq!(app.core.listened(), vec![1]);
    assert!(app.states().iter().all(is_live));

    let audio = tone(-20.0, 1.0);
    app.play(2.0, &[(1, &audio)]);
    assert_near(app.momentary(), -20.0);

    // It stays picked when a second one appears: nothing switches.
    app.list(vec![plugin(1, "Master"), plugin(2, "Kick")]);
    assert!(app.states().iter().all(is_live));
    assert_eq!(app.core.listened(), vec![1]);
    assert_eq!(app.core.pick(0).map(|p| p.id), Some(1));
}

#[test]
fn several_with_no_picks_show_the_list_and_a_click_picks() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    assert!(app.core.listened().is_empty());
    let meters = app.meters();
    for meter in &meters {
        let MeterState::PickSendPlugin(items) = &meter.state else {
            panic!("expected the list, got {:?}", meter.state);
        };
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Kick", "Bass"]);
        assert_eq!(items[0].colour, Colour::rgb(0x20, 0x80, 0xff));
        // The list sits inside the Meter, below the title.
        for item in items {
            assert!(item.frame.x >= meter.frame.x && item.frame.y > meter.frame.y);
            assert!(item.frame.y + item.frame.height <= meter.frame.y + meter.frame.height);
        }
    }
    assert_eq!(MeterState::PICK_TITLE, "Pick a Send Plugin");

    // Clicking "Bass" in the Loudness Meter's list picks it there…
    let MeterState::PickSendPlugin(items) = &meters[1].state else {
        unreachable!()
    };
    let bass = items[1].frame;
    app.send(Event::Click {
        window: WindowKey::Bar,
        at: [bass.x + bass.width / 2.0, bass.y + bass.height / 2.0],
    });
    assert_eq!(app.core.pick(1).map(|p| p.id), Some(2));
    // …and the Spectrum, with no pick, follows it.
    assert_eq!(app.core.pick(0), None);
    assert!(app.states().iter().all(is_live));
    assert_eq!(app.core.listened(), vec![2]);
}

#[test]
fn clicking_outside_the_list_picks_nothing() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.meters();
    app.send(Event::Click {
        window: WindowKey::Bar,
        at: [0.5, 0.01],
    });
    assert_eq!(app.core.pick(0), None);
    assert_eq!(app.core.pick(1), None);
}

#[test]
fn each_meter_can_show_its_own_send_plugin() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 0, id: 1 });
    app.send(Event::PickSendPlugin { meter: 1, id: 2 });
    assert_eq!(app.core.listened(), vec![1, 2]);

    let (kick, bass) = (tone(-10.0, 1.0), tone(-30.0, 1.0));
    app.play(2.0, &[(1, &kick), (2, &bass)]);
    assert_near(app.momentary(), -30.0);
}

#[test]
fn use_for_all_meters_points_every_meter_at_one_send_plugin() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 0, id: 1 });
    app.send(Event::PickSendPlugin { meter: 1, id: 2 });
    app.send(Event::UseForAllMeters { meter: 1 });
    assert_eq!(app.core.pick(0).map(|p| p.id), Some(2));
    assert_eq!(app.core.listened(), vec![2]);
}

#[test]
fn a_gone_send_plugin_is_waited_for_and_reconnects_on_its_own() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 1, id: 1 });
    app.send(Event::PickSendPlugin { meter: 0, id: 2 });

    app.change(1, |p| p.state = SendPluginState::Gone);
    let states = app.states();
    assert_eq!(states[1], MeterState::WaitingFor("Kick".to_owned()));
    assert_eq!(MeterState::waiting_text("Kick"), "Waiting for Kick");
    assert!(is_live(&states[0]), "the other Meter carries on");
    assert_eq!(
        app.core.listened(),
        vec![2],
        "nothing switches in its place"
    );

    // Removed from the table altogether: still waiting, by name.
    app.list(vec![plugin(2, "Bass")]);
    assert_eq!(app.states()[1], MeterState::WaitingFor("Kick".to_owned()));

    // Back (the DAW reopened the project): the Meter reconnects.
    app.list(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    assert!(is_live(&app.states()[1]));
    assert_eq!(app.core.listened(), vec![1, 2]);
}

#[test]
fn a_pick_is_found_again_by_name_when_its_id_is_new() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 1, id: 1 });
    app.list(vec![plugin(2, "Bass")]);
    // "Kick" comes back under another ID (the project was rebuilt).
    app.list(vec![plugin(9, "Kick"), plugin(2, "Bass")]);
    assert_eq!(app.core.pick(1).map(|p| p.id), Some(9));
    assert!(is_live(&app.states()[1]));
}

#[test]
fn a_crashed_daw_that_comes_back_is_found_by_name() {
    // The crashed DAW's slot stays listed as gone, and the reopened track
    // comes back under a new ID.
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 1, id: 1 });
    app.change(1, |p| p.state = SendPluginState::Gone);
    assert_eq!(app.states()[1], MeterState::WaitingFor("Kick".to_owned()));

    let mut listed = app.listed.clone();
    listed.push(plugin(7, "Kick"));
    app.list(listed);
    assert_eq!(app.core.pick(1).map(|p| p.id), Some(7));
    assert!(is_live(&app.states()[1]));
    assert_eq!(app.core.listened(), vec![7]);
}

#[test]
fn a_renamed_send_plugin_keeps_its_meters() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 1, id: 1 });
    app.change(1, |p| p.name = "Kick In".to_owned());
    assert_eq!(app.core.pick(1).map(|p| p.name.as_str()), Some("Kick In"));
    assert!(is_live(&app.states()[1]));
    app.change(1, |p| p.state = SendPluginState::Gone);
    assert_eq!(
        app.states()[1],
        MeterState::WaitingFor("Kick In".to_owned())
    );
}

#[test]
fn a_duplicated_track_is_a_new_send_plugin() {
    // The copy has a fresh ID and a number in its name; the pick stays put.
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 1, id: 1 });
    app.list(vec![
        plugin(1, "Kick"),
        plugin(2, "Bass"),
        plugin(3, "Kick 2"),
    ]);
    assert_eq!(app.core.pick(1).map(|p| p.id), Some(1));
    assert_eq!(app.core.listened(), vec![1]);
}

#[test]
fn an_idle_send_plugin_falls_silent_like_quiet_audio() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Master")]);
    let audio = tone(-20.0, 1.0);
    app.play(2.0, &[(1, &audio)]);
    assert_near(app.momentary(), -20.0);

    // Transport stopped: no more audio, and the transport calls it idle.
    app.change(1, |p| p.state = SendPluginState::Idle);
    app.wait(2.0);
    assert!(is_live(&app.states()[1]), "idle isn't gone: no Waiting for");
    assert_eq!(app.momentary(), Level::Silent, "momentary falls to silence");
    assert_eq!(app.core.listened(), vec![1]);
}

#[test]
fn a_sample_rate_change_resets_like_an_output_change() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Master")]);
    let audio = tone(-20.0, 1.0);
    app.play(1.0, &[(1, &audio)]);
    assert_eq!(app.sample_rate(), RATE);
    assert!(app.notes().is_empty());

    app.change(1, |p| p.sample_rate = 96_000);
    assert_eq!(app.sample_rate(), 96_000);
    assert_eq!(app.notes(), vec![Note::OutputChanged]);
}

#[test]
fn an_outdated_send_plugin_is_listed_but_cannot_be_picked() {
    let mut old = plugin(3, "Pads");
    old.outdated = true;
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass"), old]);
    let MeterState::PickSendPlugin(items) = &app.states()[0] else {
        panic!("expected the list");
    };
    assert_eq!(items[2].label, "Pads (outdated — restart your DAW)");
    assert!(!items[2].pickable);
    app.send(Event::PickSendPlugin { meter: 0, id: 3 });
    assert_eq!(app.core.pick(0), None);

    // It doesn't count as "the only one" either.
    let mut old = plugin(3, "Pads");
    old.outdated = true;
    let app = Harness::on_send_plugins(vec![old, plugin(1, "Kick")]);
    assert_eq!(app.core.listened(), vec![1]);
}

#[test]
fn picks_are_kept_on_system_capture() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 0, id: 1 });
    app.send(Event::PickSendPlugin { meter: 1, id: 2 });

    app.send(Event::SetListenTo(ListenTo::SystemCapture));
    assert!(app.core.listened().is_empty(), "Send Plugins are let go");
    assert!(app.states().iter().all(|s| *s == MeterState::Starting));
    app.send(Event::CaptureStarted {
        sample_rate: 44_100,
    });
    let audio = tone(-20.0, 1.0);
    app.send(Event::Audio(&audio));
    assert!(app.states().iter().all(is_live));

    app.send(Event::SetListenTo(ListenTo::SendPlugins));
    assert_eq!(app.core.pick(0).map(|p| p.id), Some(1));
    assert_eq!(app.core.pick(1).map(|p| p.id), Some(2));
    assert_eq!(app.core.listened(), vec![1, 2]);
}

#[test]
fn system_capture_audio_is_ignored_while_on_send_plugins() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Master")]);
    app.send(Event::CaptureStarted { sample_rate: RATE });
    let loud = tone(-6.0, 1.0);
    app.send(Event::Audio(&loud));
    // Nothing from the Send Plugin yet, so the Loudness Meter reads silence.
    assert_eq!(app.momentary(), Level::Silent);
}

#[test]
fn a_mono_send_plugin_is_flagged_on_the_stereometer() {
    let mut mono = plugin(1, "Vox");
    mono.mono = true;
    let mut app = Harness {
        core: AppCore::with_meters(vec![MeterSettings::Stereometer(Default::default())]),
        now: Duration::ZERO,
        listed: Vec::new(),
    };
    app.send(Event::SetListenTo(ListenTo::SendPlugins));
    app.list(vec![mono]);
    let MeterState::Live(MeterView::Stereometer { readings, .. }) = &app.states()[0] else {
        panic!("expected a live Stereometer");
    };
    assert!(readings.mono);
}

#[test]
fn the_source_label_names_what_a_meter_shows() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 1, id: 1 });
    assert!(
        app.meters().iter().all(|m| m.source.is_none()),
        "off by default"
    );

    app.send(Event::ShowSourceLabel {
        meter: 1,
        shown: true,
    });
    let label = app.meters()[1].source.clone();
    assert_eq!(
        label,
        Some(SourceLabel {
            name: "Kick".to_owned(),
            colour: Some(Colour::rgb(0x20, 0x80, 0xff)),
        })
    );
    assert_eq!(app.meters()[0].source, None, "only where switched on");

    app.send(Event::SetListenTo(ListenTo::SystemCapture));
    let label = app.meters()[1].source.clone();
    assert_eq!(label.map(|l| l.name), Some("System Capture".to_owned()));
}

#[test]
fn audio_from_a_send_plugin_no_meter_shows_draws_nothing() {
    let mut app = Harness::on_send_plugins(vec![plugin(1, "Kick"), plugin(2, "Bass")]);
    app.send(Event::PickSendPlugin { meter: 0, id: 1 });
    app.meters();
    app.now += Duration::from_secs(1);
    assert_eq!(app.core.decide(app.now), Decision::Sleep { until: None });
    let audio = tone(-6.0, 0.1);
    app.send(Event::SendPluginAudio {
        id: 2,
        frames: &audio,
    });
    assert_eq!(app.core.decide(app.now), Decision::Sleep { until: None });
}
