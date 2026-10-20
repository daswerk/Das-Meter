//! Hidden `--render-snapshot PATH [solo-KIND|spectrum-reference|showcase|showcase-bar|showcase-light|showcase-contrast|theme-settings|menu|settings|bar|window|zoom|dragging|light|contrast|glass|cepstrum|spectrogram|harmonics|phase-scope|phase-scope-bar|phase-scope-filled|phase-scope-lr|phase-scope-overlay|oscilloscope|oscilloscope-lr|WxH]`
//! (a PATH ending in .pam keeps the alpha channel): runs the app core on a
//! generated signal and draws its scene offscreen into a PPM image, optionally
//! with the Loudness Meter's menu or the settings panel open, or at the
//! size of a Bar on a laptop display. It checks the
//! renderers and the egui layer without a window, an audio device or
//! screen-recording permission.

use std::time::Duration;

use dasmeter_analysis::signals::{frames, pink_noise, stereo};
use dasmeter_core::{AppCore, Decision, Event, LayoutMode, Styling, WindowKey};

use crate::gpu::Gpu;
use crate::painter::{Painter, UiPaint};
use crate::ui::{Surface, Ui};

const RATE: u32 = 48_000;
const SCALE: f32 = 2.0;
const WIDTH: u32 = 2400;
const HEIGHT: u32 = 680;
/// A 180 px Bar across a 1512 px display, at 2×.
const BAR: (u32, u32) = (3024, 360);

/// A kick on every beat at 120 BPM over a bass line (an octave jump each
/// bar), `seconds` long, mono.
fn kick_and_bass(seconds: f64) -> Vec<f32> {
    let (kick, bass) = kick_and_bass_apart(seconds);
    kick.iter().zip(&bass).map(|(k, b)| k + b).collect()
}

/// [`kick_and_bass`] as two tracks: the kick, and the bass.
fn kick_and_bass_apart(seconds: f64) -> (Vec<f32>, Vec<f32>) {
    let beat = f64::from(RATE) / 2.0;
    let (mut kick_phase, mut bass_phase) = (0.0f64, 0.0f64);
    (0..frames(RATE, seconds))
        .map(|i| {
            let t = (i as f64 % beat) / f64::from(RATE);
            if i as f64 % beat < 1.0 {
                kick_phase = 0.0;
            }
            kick_phase +=
                std::f64::consts::TAU * (45.0 + 110.0 * (-t * 35.0).exp()) / f64::from(RATE);
            let kick = kick_phase.sin() * (-t * 7.0).exp() * 0.7;
            let bar = (i as f64 / (beat * 4.0)).floor() as i64;
            let bass_freq = if bar % 2 == 0 { 55.0 } else { 41.2 };
            bass_phase += std::f64::consts::TAU * bass_freq / f64::from(RATE);
            let bass = bass_phase.sin() * 0.3 * (1.0 - (-t * 20.0).exp());
            (kick as f32, bass as f32)
        })
        .unzip()
}

/// Something like music for the README's screenshots, `seconds` long and
/// interleaved stereo: [`kick_and_bass`] in the middle, noise hats on the
/// off-beats, and an A minor pad detuned a little between Left and Right so
/// the Stereometer has width to show.
fn music(seconds: f64) -> Vec<f32> {
    let n = frames(RATE, seconds);
    let low = kick_and_bass(seconds);
    let (left, right) = (pink_noise(n, -10.0, 11), pink_noise(n, -10.0, 12));
    let eighth = f64::from(RATE) / 4.0;
    let pad = |t: f64, detune: f64, phase: f64| -> f32 {
        [220.0, 261.63, 329.63, 440.0]
            .iter()
            .map(|f| (std::f64::consts::TAU * (f + detune) * t + phase).sin() * 0.06)
            .sum::<f64>() as f32
    };
    (0..n)
        .flat_map(|i| {
            let t = i as f64 / f64::from(RATE);
            let into = i as f64 % (2.0 * eighth);
            // Off-beat eighths only, each decaying fast.
            let hat = if into >= eighth {
                (-(into - eighth) / f64::from(RATE) * 40.0).exp() as f32
            } else {
                0.0
            };
            let mid = 0.55 * low[i];
            [
                mid + hat * left[i] + pad(t, -0.7, 0.0),
                mid + hat * right[i] + pad(t, 0.7, 1.3),
            ]
        })
        .collect()
}

/// A harmonic tone at `pitch` Hz, `seconds` long, interleaved stereo: what
/// the Cepstrum and Oscilloscope show best.
fn tone(pitch: f64, partials: &[(i32, f64)], seconds: f64) -> Vec<f32> {
    (0..frames(RATE, seconds))
        .flat_map(|i| {
            let t = i as f64 / f64::from(RATE);
            let x: f64 = partials
                .iter()
                .map(|&(k, a)| a * (std::f64::consts::TAU * pitch * f64::from(k) * t).sin())
                .sum();
            let x = x as f32;
            [x, x]
        })
        .collect()
}

/// Feeds `audio` (interleaved stereo) to the core as System Capture would.
fn play(core: &mut AppCore, audio: &[f32], mut now: Duration) -> Duration {
    for block in audio.chunks(1024) {
        now += Duration::from_secs_f64(512.0 / f64::from(RATE));
        core.handle(Event::Audio(block), now);
        core.decide(now);
    }
    now
}

/// Meter `meter` as a Phase Scope on a Kick Send Plugin with the Bass from
/// the same DAW as its Overlay Source, both following the DAW at 120 BPM,
/// with suggestions on; `split` puts the sum in its own row, `fit` plays a
/// Bass that fits.
fn kick_with_bass_overlay(
    core: &mut AppCore,
    meter: usize,
    split: bool,
    fit: bool,
    mut now: Duration,
) -> Duration {
    let settings =
        dasmeter_core::MeterSettings::PhaseScope(dasmeter_core::PhaseScopeMeterSettings {
            suggestions: true,
            split_sum: split,
            ..Default::default()
        });
    core.handle(Event::SetMeter { meter, settings }, now);
    core.handle(
        Event::SetListenTo(dasmeter_core::ListenTo::SendPlugins),
        now,
    );
    let plugin = |id: u64, name: &str, colour: u32| dasmeter_core::SendPlugin {
        id,
        name: name.to_owned(),
        colour,
        mono: false,
        sample_rate: RATE,
        state: dasmeter_core::SendPluginState::Live,
        outdated: false,
        host_pid: 1,
    };
    let listed = [plugin(1, "Kick", 0xff_6a_3d), plugin(2, "Bass", 0x4d_a3_ff)];
    core.handle(Event::SendPlugins(&listed), now);
    core.handle(Event::PickSendPlugin { meter, id: 1 }, now);
    core.handle(Event::PickOverlay { meter, id: Some(2) }, now);
    let (kick, mut bass) = kick_and_bass_apart(8.0);
    if fit {
        bass = kick.iter().map(|k| k * 0.6).collect();
    }
    for (block, (kick, bass)) in kick.chunks(512).zip(bass.chunks(512)).enumerate() {
        let beats = (block * 512) as f64 / f64::from(RATE) * 2.0;
        let timing = Some(dasmeter_core::Timing {
            tempo: 120.0,
            beats,
            bar_start: (beats / 4.0).floor() * 4.0,
            signature: (4, 4),
            playing: true,
        });
        now += Duration::from_secs_f64(512.0 / f64::from(RATE));
        for (id, mono) in [(1, kick), (2, bass)] {
            let frames: Vec<f32> = mono.iter().flat_map(|&x| [x, x]).collect();
            core.handle(
                Event::SendPluginAudio {
                    id,
                    frames: &frames,
                    timing,
                },
                now,
            );
        }
        core.decide(now);
    }
    now
}

/// Window mode with only one Meter, of `kind` ("waveform", "phase-scope"…),
/// on a signal that shows it well.
fn solo(core: &mut AppCore, kind: &str, now: Duration) -> Result<Duration, String> {
    use dasmeter_core::MeterKind;
    let meter_kind = match kind {
        "waveform" => MeterKind::Waveform,
        "spectrum" => MeterKind::Spectrum,
        "spectrogram" => MeterKind::Spectrogram,
        "loudness" => MeterKind::Loudness,
        "stereometer" => MeterKind::Stereometer,
        "phase-scope" => MeterKind::PhaseScope,
        "cepstrum" => MeterKind::Cepstrum,
        "oscilloscope" => MeterKind::Oscilloscope,
        "tonal-balance" => MeterKind::TonalBalance,
        "tuner" => MeterKind::Tuner,
        "stereo-bands" => MeterKind::StereoBands,
        other => return Err(format!("no Meter kind {other:?}")),
    };
    core.handle(Event::SetMode(LayoutMode::Window), now);
    core.handle(Event::StartListening, now);
    core.handle(
        Event::AssignMeter {
            meter: 0,
            kind: meter_kind,
        },
        now,
    );
    for meter in (1..8).rev() {
        core.handle(Event::ClosePane { meter }, now);
    }
    core.handle(Event::Pointer(None), now);
    Ok(match meter_kind {
        MeterKind::PhaseScope => kick_with_bass_overlay(core, 0, false, false, now),
        MeterKind::Cepstrum => {
            let partials: Vec<(i32, f64)> = (1..=100).map(|k| (k, 0.15 / f64::from(k))).collect();
            play(core, &tone(220.0, &partials, 2.0), now)
        }
        MeterKind::Oscilloscope => {
            let partials = [(1, 0.2), (2, 0.07), (3, 0.04), (5, 0.016)];
            play(core, &tone(220.0, &partials, 2.0), now)
        }
        // The music, its bass mono and its hats wide, with a pad whose
        // Right is flipped above 2 kHz: something in every row.
        MeterKind::StereoBands => {
            let mut audio = music(3.0);
            let wide = pink_noise(audio.len() / 2, -24.0, 7);
            for (frame, w) in audio.chunks_mut(2).zip(&wide) {
                frame[0] += w;
                frame[1] -= w;
            }
            play(core, &audio, now)
        }
        // A bass note 7 cents sharp of A2.
        MeterKind::Tuner => {
            let partials: Vec<(i32, f64)> = (1..=12).map(|k| (k, 0.2 / f64::from(k))).collect();
            play(
                core,
                &tone(110.0 * 2f64.powf(7.0 / 1_200.0), &partials, 2.0),
                now,
            )
        }
        _ => play(core, &music(10.0), now),
    })
}

fn tonal_balance(core: &mut AppCore, variant: &str, now: Duration) -> Result<Duration, String> {
    use dasmeter_core::{MeterSettings, TonalView};
    let mut now = solo(core, "tonal-balance", now)?;
    let Some(MeterSettings::TonalBalance(mut settings)) = core.meter_settings(0) else {
        return Err("no Tonal Balance Meter".into());
    };
    settings.averaging = dasmeter_analysis::LongTerm::Whole;
    settings.view = match variant {
        "-broad" => TonalView::Broad,
        _ => TonalView::Fine,
    };
    settings.low_end_zoom = variant == "-low" || variant == "-shift";
    if variant == "-shift" {
        settings.key = dasmeter_core::KeyChoice::Fixed(dasmeter_analysis::tonal_balance::Key {
            tonic: 5,
            minor: true,
        });
    }
    core.handle(
        Event::SetMeter {
            meter: 0,
            settings: MeterSettings::TonalBalance(settings),
        },
        now,
    );
    // The Target's song: the music darker, with its lows lifted.
    let darker: Vec<f32> = {
        let mut state = [0.0f32; 2];
        music(10.0)
            .chunks(2)
            .flat_map(|frame| {
                let mut out = [0.0f32; 2];
                for ((s, &x), o) in state.iter_mut().zip(frame).zip(&mut out) {
                    *s += 0.02 * (x - *s);
                    *o = 0.7 * x + 2.0 * *s;
                }
                out
            })
            .collect()
    };
    core.handle(Event::LearnSong { meter: 0 }, now);
    now = play(core, &darker, now);
    core.handle(
        Event::AddLearnedSong {
            meter: 0,
            target: "My references",
            song: "Darker mix",
        },
        now,
    );
    core.handle(Event::ResetLongTerm { meter: 0 }, now);
    now = play(core, &music(10.0), now);
    // Its menu, learning another song.
    if variant == "-menu" {
        core.handle(Event::LearnSong { meter: 0 }, now);
        now = play(core, &music(3.0), now);
        core.handle(
            Event::OpenMenu {
                window: WindowKey::Main,
                at: [0.5, 0.5],
            },
            now,
        );
    }
    Ok(now)
}

/// DASMETER_THEME="Name": use that Theme. DASMETER_STYLE="key=value,…" (keys:
/// look, glow, fade, gradient, vignette, dim, opacity, line, gap, radius,
/// text, cues): change the Theme in use (a built-in becomes an edited copy).
fn theme_from_env(core: &mut AppCore, now: Duration) -> Result<(), String> {
    fn themes(core: &mut AppCore, now: Duration) -> Result<dasmeter_core::ThemeScene, String> {
        core.decide(now);
        Ok(core.scene().ok_or("no scene")?.theme.clone())
    }
    if let Some(name) = std::env::var_os("DASMETER_THEME") {
        let name = name.to_string_lossy();
        let scene = themes(core, now)?;
        let index = scene
            .themes
            .iter()
            .position(|t| t.name == name)
            .ok_or_else(|| format!("no Theme {name:?}"))?;
        core.handle(
            Event::ChooseTheme {
                light: index,
                dark: index,
            },
            now,
        );
    }
    if let Some(style) = std::env::var_os("DASMETER_STYLE") {
        let scene = themes(core, now)?;
        let current = scene.current.ok_or("no Theme in use")?;
        let mut styling = scene.styling;
        for pair in style.to_string_lossy().split(',') {
            let (key, value) = pair.split_once('=').ok_or("DASMETER_STYLE: key=value")?;
            let number = || {
                value
                    .parse::<f32>()
                    .map_err(|_| format!("{key}: not a number"))
            };
            let on = || value == "on";
            match key.trim() {
                "look" if value == "classic" => styling.look = dasmeter_core::Look::Classic,
                "look" => styling.look = dasmeter_core::Look::Smooth,
                "glow" => styling.glow = number()?,
                "fade" => styling.grid_fade = number()?,
                "gradient" => styling.gradient = number()?,
                "vignette" => styling.vignette = number()?,
                "dim" => styling.dim_when_silent = on(),
                "opacity" => styling.background_opacity = number()?,
                "line" => {
                    styling.line = match value {
                        "thin" => dasmeter_core::LineWeight::Thin,
                        "thick" => dasmeter_core::LineWeight::Thick,
                        _ => dasmeter_core::LineWeight::Normal,
                    }
                }
                "gap" => styling.gap = number()?,
                "radius" => styling.corner_radius = number()?,
                "text" => styling.text_scale = number()?,
                "cues" => styling.shape_cues = on(),
                other => return Err(format!("DASMETER_STYLE: no key {other:?}")),
            }
        }
        core.handle(
            Event::SetThemeStyling {
                theme: current,
                styling,
            },
            now,
        );
        core.take_writes();
        // Editing a built-in switches to its edited copy by itself.
    }
    Ok(())
}

pub fn render(path: &str, open: Option<&str>) -> Result<(), String> {
    // Six seconds of pink noise with an output change after four, so the
    // snapshot shows levels, loudness and the "Output changed" note.
    let mut core = AppCore::new();
    let noise = |seconds, seed| {
        let n = frames(RATE, seconds);
        stereo(&pink_noise(n, -12.0, seed), &pink_noise(n, -15.0, seed + 1))
    };
    let mut now = Duration::ZERO;
    // DASMETER_CLASSIC=1: Dark drawn Classic as the app used to draw it, to
    // compare with snapshots from before the Smooth Look. Set before any
    // audio, so the Meters see the same frames as they did then.
    if std::env::var_os("DASMETER_CLASSIC").is_some() {
        let styling = Styling {
            look: dasmeter_core::Look::Classic,
            corner_radius: 4.0,
            ..Styling::default()
        };
        core.handle(Event::SetThemeStyling { theme: 0, styling }, now);
        core.take_writes();
    }
    core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
    // The silence hint only shows when nothing has played.
    let noise_for = match open {
        Some(option)
            if option.starts_with("solo-")
                || option.starts_with("tonal-balance")
                || option.starts_with("loudness-")
                || option == "spectrum-reference" =>
        {
            0
        }
        Some(
            "hint" | "showcase" | "showcase-bar" | "showcase-light" | "showcase-contrast"
            | "theme-settings",
        ) => 0,
        _ => 2,
    };
    for (seconds, seed) in [(4.0, 1), (2.0, 3)].into_iter().take(noise_for) {
        for block in noise(seconds, seed).chunks(1024) {
            now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            core.handle(Event::Audio(block), now);
        }
        if seed == 1 {
            core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
        }
    }
    // The cursor over the Spectrum, for its readout.
    core.handle(Event::Pointer(Some((WindowKey::Bar, [0.4, 0.5]))), now);
    if core.decide(now) != Decision::Draw {
        return Err("the core had nothing to draw".into());
    }
    theme_from_env(&mut core, now)?;
    // "WxH": a window of that many logical px, for checking small sizes;
    // "option@WxH" for an option at that size.
    let (open, size) = match open.and_then(|o| o.split_once('@')) {
        Some((option, size)) => (Some(option), Some(size)),
        None => (open, open),
    };
    let custom = size.and_then(|o| {
        let (w, h) = o.split_once('x')?;
        Some((w.parse::<u32>().ok()? * 2, h.parse::<u32>().ok()? * 2))
    });
    let (width, height) = match (open, custom) {
        (_, Some(size)) => size,
        (Some("bar" | "showcase-bar"), _) => BAR,
        (
            Some(
                "window" | "zoom" | "dragging" | "bars-window" | "spectrogram-zoom" | "showcase"
                | "showcase-light" | "showcase-contrast",
            ),
            _,
        ) => (2400, 1440),
        (Some("theme-settings"), _) => (2400, 1800),
        (Some(option), _)
            if option.starts_with("solo-")
                || option.starts_with("tonal-balance")
                || option.starts_with("loudness-")
                || option == "spectrum-reference" =>
        {
            (1440, 900)
        }
        _ => (WIDTH, HEIGHT),
    };
    match open {
        None | Some("bar") => {}
        // For the README: Window mode with six Meters on music, or the
        // settings open on the Theme page.
        Some("showcase-bar") => {
            core.handle(Event::StartListening, now);
            core.handle(Event::Pointer(None), now);
            for block in music(10.0).chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        Some(option @ ("showcase" | "showcase-light" | "showcase-contrast" | "theme-settings")) => {
            core.handle(Event::SetMode(LayoutMode::Window), now);
            core.handle(Event::StartListening, now);
            match option {
                "showcase-light" => core.handle(Event::ChooseTheme { light: 1, dark: 1 }, now),
                "showcase-contrast" => core.handle(Event::ChooseTheme { light: 2, dark: 2 }, now),
                _ => {}
            }
            if option != "theme-settings" {
                // Meter 0 is the Waveform, 1 the Spectrum.
                core.handle(
                    Event::AddMeter {
                        meter: 1,
                        kind: dasmeter_core::MeterKind::Spectrogram,
                    },
                    now,
                );
                core.handle(
                    Event::AddMeter {
                        meter: 0,
                        kind: dasmeter_core::MeterKind::PhaseScope,
                    },
                    now,
                );
            } else {
                core.handle(Event::ShowSettings(true), now);
            }
            core.handle(Event::Pointer(None), now);
            for block in music(10.0).chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        Some("window") => {
            core.handle(Event::SetMode(LayoutMode::Window), now);
            core.handle(Event::Pointer(Some((WindowKey::Main, [0.4, 0.25]))), now);
        }
        // A box dragged over the Spectrum (2 to 8 kHz or so), released into
        // the zoom window or still being dragged.
        Some(option @ ("zoom" | "dragging")) => {
            core.handle(Event::SetMode(LayoutMode::Window), now);
            // Past the first-launch "Start listening", which a click would press.
            core.handle(Event::StartListening, now);
            now += Duration::from_millis(100);
            core.decide(now);
            let window = WindowKey::Main;
            core.handle(
                Event::Click {
                    window,
                    at: [0.66, 0.12],
                },
                now,
            );
            core.handle(Event::Pointer(Some((window, [0.86, 0.3]))), now);
            if option == "zoom" {
                core.handle(
                    Event::Release {
                        window,
                        at: [0.86, 0.3],
                    },
                    now,
                );
                // The pointer in the zoom window, for its pitch readout.
                core.handle(Event::Pointer(Some((window, [0.25, 0.25]))), now);
            }
            for block in noise(1.0, 5).chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
            }
        }
        Some(_) if custom.is_some() && size == open => {}
        // The Loudness Meter with its bars on, in the Bar or a window.
        Some(option @ ("bars" | "bars-window")) => {
            if option == "bars-window" {
                core.handle(Event::SetMode(LayoutMode::Window), now);
            }
            let meter = (0..8)
                .find(|&i| {
                    matches!(
                        core.meter_settings(i),
                        Some(dasmeter_core::MeterSettings::Loudness(_))
                    )
                })
                .ok_or("there's no Loudness Meter")?;
            let Some(dasmeter_core::MeterSettings::Loudness(mut settings)) =
                core.meter_settings(meter)
            else {
                unreachable!()
            };
            settings.show_bars = true;
            core.handle(
                Event::SetMeter {
                    meter,
                    settings: dasmeter_core::MeterSettings::Loudness(settings),
                },
                now,
            );
            for block in noise(1.0, 7).chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
            }
        }
        // Themes: the built-ins, and a see-through copy of Dark.
        Some("light") => core.handle(Event::ChooseTheme { light: 1, dark: 1 }, now),
        Some("contrast") => core.handle(Event::ChooseTheme { light: 2, dark: 2 }, now),
        Some("glass") => {
            core.handle(Event::DuplicateTheme { theme: 0 }, now);
            let styling = Styling {
                background_opacity: 0.55,
                corner_radius: 10.0,
                gap: 8.0,
                ..Styling::default()
            };
            let copy = dasmeter_core::Theme::built_ins().len();
            core.handle(
                Event::SetThemeStyling {
                    theme: copy,
                    styling,
                },
                now,
            );
            core.take_writes();
        }
        // Right-click the Loudness Meter, as a user would.
        Some("menu") => core.handle(
            Event::OpenMenu {
                window: WindowKey::Bar,
                at: [0.8, 0.1],
            },
            now,
        ),
        Some("settings") => core.handle(Event::ShowSettings(true), now),
        // The Spectrogram in the Spectrum's place, on a sweep from 60 Hz to
        // 12 kHz over pink noise, with a 1 kHz tone joining halfway; or, in a
        // window, zoomed into the box around the tone.
        Some(option @ ("spectrogram" | "spectrogram-zoom")) => {
            let settings =
                dasmeter_core::MeterSettings::default_of(dasmeter_core::MeterKind::Spectrogram);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            if option == "spectrogram-zoom" {
                core.handle(Event::SetMode(LayoutMode::Window), now);
                core.handle(Event::StartListening, now);
                now += Duration::from_millis(100);
                core.decide(now);
                let window = WindowKey::Main;
                core.handle(
                    Event::Click {
                        window,
                        at: [0.55, 0.2],
                    },
                    now,
                );
                core.handle(Event::Pointer(Some((window, [0.95, 0.3]))), now);
                core.handle(
                    Event::Release {
                        window,
                        at: [0.95, 0.3],
                    },
                    now,
                );
            }
            let seconds = 10.0;
            let n = frames(RATE, seconds);
            let noise = pink_noise(n, -40.0, 9);
            let (f0, f1) = (60.0_f64, 12_000.0_f64);
            let rate = (f1 / f0).ln() / seconds;
            let audio: Vec<f32> = (0..n)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let phase = std::f64::consts::TAU * f0 * ((rate * t).exp() - 1.0) / rate;
                    let mut x = 0.25 * phase.sin() + f64::from(noise[i]);
                    if t > seconds / 2.0 {
                        x += 0.1 * (std::f64::consts::TAU * 1_000.0 * t).sin();
                    }
                    [x as f32, x as f32]
                })
                .collect();
            for block in audio.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        // The Spectrum coloured by steady harmonics, on a 55 Hz bass tone
        // with its harmonics over pink noise, the pointer over it.
        Some("harmonics") => {
            let Some(dasmeter_core::MeterSettings::Spectrum(mut settings)) = core.meter_settings(1)
            else {
                return Err("the second Meter isn't a Spectrum".into());
            };
            settings.colouring = dasmeter_core::SpectrumColouring::SteadyHarmonics;
            core.handle(
                Event::SetMeter {
                    meter: 1,
                    settings: dasmeter_core::MeterSettings::Spectrum(settings),
                },
                now,
            );
            let n = frames(RATE, 4.0);
            let noise = pink_noise(n, -36.0, 11);
            let audio: Vec<f32> = (0..n)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let x: f64 = (1..=8)
                        .map(|k| {
                            let k = f64::from(k);
                            (std::f64::consts::TAU * 55.0 * k * t).sin() * 0.3 / k
                        })
                        .sum::<f64>()
                        + f64::from(noise[i]);
                    [x as f32, x as f32]
                })
                .collect();
            for block in audio.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
            core.handle(Event::Pointer(Some((WindowKey::Bar, [0.3, 0.5]))), now);
        }
        // The Cepstrum in the Spectrum's place, on a 220 Hz harmonic tone,
        // with the pointer over it.
        Some("cepstrum") => {
            let settings =
                dasmeter_core::MeterSettings::default_of(dasmeter_core::MeterKind::Cepstrum);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            let partials = 100;
            let tone: Vec<f32> = (0..RATE as usize)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let x: f64 = (1..=partials)
                        .map(|k| {
                            (std::f64::consts::TAU * 220.0 * f64::from(k) * t).sin() / f64::from(k)
                        })
                        .sum();
                    let x = (0.15 * x) as f32;
                    [x, x]
                })
                .collect();
            for block in tone.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
            core.handle(Event::Pointer(Some((WindowKey::Bar, [0.4, 0.5]))), now);
        }
        // The Oscilloscope in the Spectrum's place, on a 220 Hz tone with a
        // few harmonics; `oscilloscope-lr` shows Left and Right, the right
        // quieter and a little late.
        Some(option @ ("oscilloscope" | "oscilloscope-lr")) => {
            let mut settings = dasmeter_core::OscilloscopeMeterSettings::default();
            let lr = option == "oscilloscope-lr";
            if lr {
                settings.analysis.channels = dasmeter_analysis::ChannelView::LeftRight;
            }
            let settings = dasmeter_core::MeterSettings::Oscilloscope(settings);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            let wave = |t: f64| -> f32 {
                let x: f64 = [(1, 1.0), (2, 0.35), (3, 0.2), (5, 0.08)]
                    .iter()
                    .map(|&(k, a)| a * (std::f64::consts::TAU * 220.0 * f64::from(k) * t).sin())
                    .sum();
                (0.2 * x) as f32
            };
            let tone: Vec<f32> = (0..RATE as usize)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let right = if lr { 0.6 * wave(t - 0.0006) } else { wave(t) };
                    [wave(t), right]
                })
                .collect();
            for block in tone.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        // The Phase Scope in the Spectrum's place, on a kick and a bass at
        // 120 BPM: the typed-in tempo, a beat or (`phase-scope-bar`) a bar,
        // filled, or Left and Right (the right channel quieter).
        Some(
            option @ ("phase-scope" | "phase-scope-bar" | "phase-scope-filled" | "phase-scope-lr"),
        ) => {
            let mut settings = dasmeter_core::PhaseScopeMeterSettings::default();
            match option {
                "phase-scope-bar" => settings.cycle = dasmeter_core::CycleLength::Bar,
                "phase-scope-filled" => settings.filled = true,
                "phase-scope-lr" => {
                    settings.channel_view = dasmeter_analysis::ChannelView::LeftRight;
                }
                _ => {}
            }
            let right = if option == "phase-scope-lr" { 0.5 } else { 1.0 };
            let settings = dasmeter_core::MeterSettings::PhaseScope(settings);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            let audio: Vec<f32> = kick_and_bass(8.0)
                .into_iter()
                .flat_map(|x| [x, x * right])
                .collect();
            for block in audio.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        // The Phase Scope on a Kick Send Plugin with the Bass from the same
        // DAW as its Overlay Source, both following the DAW at 120 BPM,
        // with suggestions on; `phase-scope-split` with the sum in its own
        // row, `phase-scope-fit` with a Bass that fits.
        Some(option @ ("phase-scope-overlay" | "phase-scope-split" | "phase-scope-fit")) => {
            now = kick_with_bass_overlay(
                &mut core,
                1,
                option == "phase-scope-split",
                option == "phase-scope-fit",
                now,
            );
        }
        // A Spectrum alone with its long-term average (10 s) and a Reference
        // curve saved from other, darker audio.
        Some("spectrum-reference") => {
            now = solo(&mut core, "spectrum", now)?;
            let Some(dasmeter_core::MeterSettings::Spectrum(mut settings)) = core.meter_settings(0)
            else {
                return Err("no Spectrum".into());
            };
            settings.analysis.long_term = dasmeter_analysis::LongTerm::Seconds(10.0);
            core.handle(
                Event::SetMeter {
                    meter: 0,
                    settings: dasmeter_core::MeterSettings::Spectrum(settings),
                },
                now,
            );
            // The reference: the music with its highs pulled down.
            let darker: Vec<f32> = {
                let mut state = [0.0f32; 2];
                music(10.0)
                    .chunks(2)
                    .flat_map(|frame| {
                        for (s, &x) in state.iter_mut().zip(frame) {
                            *s += 0.08 * (x - *s);
                        }
                        [state[0] * 1.6, state[1] * 1.6]
                    })
                    .collect()
            };
            now = play(&mut core, &darker, now);
            core.handle(
                Event::SaveReferenceCurve {
                    meter: 0,
                    name: "Reference track",
                },
                now,
            );
            core.handle(Event::ResetLongTerm { meter: 0 }, now);
            now = play(&mut core, &music(10.0), now);
        }
        // A Tonal Balance Meter alone, its Target taught with the music
        // made darker and with more bass, then the music played against it;
        // "-broad" and "-low" for those views, "-shift" with the Target
        // shifted to a fixed key.
        Some(option) if option.starts_with("tonal-balance") => {
            now = tonal_balance(&mut core, &option["tonal-balance".len()..], now)?;
        }
        // The Loudness Meter alone with its bars on a scale: "loudness-scale-vu",
        // "-ppm-din", "-ppm-nordic", "-ppm-bbc", "-ppm-ebu", "-k12", "-k14", "-k20".
        Some(option) if option.starts_with("loudness-scale-") => {
            use dasmeter_analysis::Scale;
            let scale = match &option["loudness-scale-".len()..] {
                "vu" => Scale::Vu,
                "ppm-din" => Scale::PpmDin,
                "ppm-nordic" => Scale::PpmNordic,
                "ppm-bbc" => Scale::PpmBbc,
                "ppm-ebu" => Scale::PpmEbu,
                "k12" => Scale::K12,
                "k14" => Scale::K14,
                "k20" => Scale::K20,
                other => return Err(format!("no scale {other:?}")),
            };
            now = solo(&mut core, "loudness", now)?;
            let Some(dasmeter_core::MeterSettings::Loudness(mut settings)) = core.meter_settings(0)
            else {
                return Err("no Loudness Meter".into());
            };
            settings.show_bars = true;
            settings.analysis.scale = scale;
            core.handle(
                Event::SetMeter {
                    meter: 0,
                    settings: dasmeter_core::MeterSettings::Loudness(settings),
                },
                now,
            );
            now = play(&mut core, &music(3.0), now);
        }
        // The Loudness Meter alone after music with a few clipped hits.
        Some("loudness-counters") => {
            now = solo(&mut core, "loudness", now)?;
            let mut loud = music(4.0);
            for hit in 0..3 {
                let start = (hit * 2 * RATE as usize / 3) * 2 + 4_000;
                for s in &mut loud[start..start + 400] {
                    *s = (*s * 6.0).clamp(-1.0, 1.0);
                }
            }
            now = play(&mut core, &loud, now);
        }
        // One Meter alone in a window, on a signal that suits it.
        Some(option) if option.starts_with("solo-") => {
            now = solo(&mut core, &option["solo-".len()..], now)?;
        }
        // The first-launch card, and the silence hint after 11 s of nothing.
        Some("welcome") => core.handle(Event::ShowWelcome, now),
        Some("hint") => {
            core.handle(Event::StartListening, now);
            let silence = vec![0.0f32; 2 * 512];
            for _ in 0..(11 * RATE / 512) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(&silence), now);
            }
        }
        Some(other) => return Err(format!("unknown option {other:?}")),
    }
    // DASMETER_OVERRIDE="role=#RRGGBB,…": per-Meter colours on the first Meter.
    if let Some(overrides) = std::env::var_os("DASMETER_OVERRIDE") {
        for pair in overrides.to_string_lossy().split(',') {
            let (key, hex) = pair
                .split_once('=')
                .ok_or("DASMETER_OVERRIDE: role=#RRGGBB")?;
            let role = dasmeter_core::Role::ALL
                .into_iter()
                .find(|r| r.key() == key.trim())
                .ok_or_else(|| format!("no colour role {key:?}"))?;
            let colour = dasmeter_core::Colour::from_hex(hex.trim())
                .ok_or_else(|| format!("not a colour: {hex:?}"))?;
            core.handle(
                Event::SetOverride {
                    meter: 0,
                    role,
                    colour: Some(colour),
                },
                now,
            );
        }
    }
    now += Duration::from_millis(100);
    core.decide(now);

    let gpu = pollster::block_on(Gpu::offscreen(width, height))?;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut painter = Painter::new(gpu);

    // egui lays windows out over a few frames; the last one is drawn.
    let mut ui = Ui::new();
    ui.use_fonts(painter.text.font_system.db());
    if open == Some("theme-settings") {
        ui.show_theme_page();
    }
    let scene = core.scene().ok_or("no scene")?;
    let mut output = None;
    for frame in 0..4 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32 / SCALE, height as f32 / SCALE),
            )),
            time: Some(f64::from(frame) * 0.5),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(SCALE);
        let (mut out, _) = ui.run(input, scene, Surface::Overlay);
        // Textures made in earlier frames must reach the painter too.
        if let Some(previous) = output.take() {
            let previous: egui::FullOutput = previous;
            let mut textures = previous.textures_delta;
            textures.append(out.textures_delta);
            out.textures_delta = textures;
        }
        output = Some(out);
    }
    let ui = output.map(|output| UiPaint::new(&ui.ctx, output));

    painter.paint(
        core.scene(),
        core.scene()
            .and_then(|scene| scene.windows.first())
            .map(|w| w.key),
        SCALE,
        &texture.create_view(&wgpu::TextureViewDescriptor::default()),
        ui,
    );

    let gpu = &painter.gpu;
    let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    gpu.queue.submit(Some(encoder.finish()));
    buffer.map_async(wgpu::MapMode::Read, .., |result| {
        result.expect("map the snapshot");
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    let pixels = buffer.get_mapped_range(..).map_err(|e| e.to_string())?;

    // A .pam file keeps the alpha channel (for see-through Themes); else PPM.
    let alpha = path.ends_with(".pam");
    let mut ppm = if alpha {
        format!(
            "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
        )
        .into_bytes()
    } else {
        format!("P6\n{width} {height}\n255\n").into_bytes()
    };
    for y in 0..height as usize {
        let start = y * row as usize;
        let (line, _) = pixels[start..start + width as usize * 4].as_chunks::<4>();
        for pixel in line {
            ppm.extend_from_slice(if alpha { &pixel[..] } else { &pixel[..3] });
        }
    }
    std::fs::write(path, ppm).map_err(|e| e.to_string())
}
