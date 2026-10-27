//! The Tonal Balance Meter's analysis (#105).

use dasmeter_analysis::LongTerm;
use dasmeter_analysis::signals::{both, pink_noise, silence, sine, stereo};
use dasmeter_analysis::tonal_balance::{
    FINE_BELOW_HZ, Frame, FrameAnalyser, Key, LiveSummary, Parts, Region, Summary, detect_key,
    fine_points, shift_low_end, summarise, tonal_grid,
};

const RATE: u32 = 48_000;

fn frames_of(audio: &[f32]) -> Vec<Frame> {
    let mut analyser = FrameAnalyser::new(RATE, 120.0);
    analyser.process(audio);
    analyser.take_frames()
}

fn summary_of(audio: &[f32]) -> Summary {
    summarise(&frames_of(audio), Parts::Whole).expect("something was heard")
}

fn seconds(s: f64) -> usize {
    (s * f64::from(RATE)) as usize
}

fn pink(s: f64, dbfs: f64, seed: u64) -> Vec<f32> {
    let n = seconds(s);
    stereo(&pink_noise(n, dbfs, seed), &pink_noise(n, dbfs, seed + 1))
}

fn add(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x + y).collect()
}

#[test]
fn the_grid_is_fine_in_the_low_end() {
    let grid = tonal_grid();
    assert_eq!(grid[0], 20.0);
    let fine = fine_points();
    assert!(grid[fine - 1] < FINE_BELOW_HZ && grid[fine] >= FINE_BELOW_HZ);
    // Two points per semitone below 300 Hz.
    let step = (grid[1] / grid[0]).log2() * 12.0;
    assert!((step - 0.5).abs() < 1e-4, "step {step} semitones");
    assert!(*grid.last().unwrap() <= 20_000.0 * 1.13);
}

#[test]
fn the_sub_and_bass_notes_are_found() {
    // E1 (41.20 Hz) in the Sub, A1 (110 Hz) in the Bass.
    let audio = both(&add(
        &sine(RATE, 41.203, -12.0, 0.0, seconds(6.0)),
        &sine(RATE, 110.0, -18.0, 0.0, seconds(6.0)),
    ));
    let summary = summary_of(&audio);
    let sub = summary.sub.expect("a Sub note");
    assert_eq!(sub.note.to_string(), "E1");
    assert!(
        (sub.frequency - 41.2).abs() < 0.6,
        "Sub at {} Hz",
        sub.frequency
    );
    let bass = summary.bass.expect("a Bass note");
    assert_eq!(bass.note.to_string(), "A2");
    assert!(
        (bass.frequency - 110.0).abs() < 1.5,
        "Bass at {} Hz",
        bass.frequency
    );
}

#[test]
fn pink_noise_reads_flat() {
    // Noise scatters from point to point (a 1/24 octave holds few bins), so
    // compare third-octave averages: the curve has no tilt or bias.
    let summary = summary_of(&pink(8.0, -20.0, 3));
    let grid = tonal_grid();
    let third = |centre: f32| -> f32 {
        let (low, high) = (centre / 2f32.powf(1.0 / 6.0), centre * 2f32.powf(1.0 / 6.0));
        let levels: Vec<f32> = grid
            .iter()
            .zip(&summary.curve)
            .filter(|(f, _)| (low..high).contains(*f))
            .map(|(_, &l)| 10f32.powf(l / 10.0))
            .collect();
        10.0 * (levels.iter().sum::<f32>() / levels.len() as f32).log10()
    };
    let thirds: Vec<f32> = [
        50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0,
    ]
    .into_iter()
    .map(third)
    .collect();
    let mean = thirds.iter().sum::<f32>() / thirds.len() as f32;
    let worst = thirds.iter().map(|l| (l - mean).abs()).fold(0.0, f32::max);
    assert!(
        worst < 1.5,
        "pink noise strays {worst:.2} dB from flat: {thirds:?}"
    );
}

#[test]
fn the_curve_is_shape_only() {
    let loud = summary_of(&pink(6.0, -10.0, 5));
    let quiet = summary_of(&pink(6.0, -30.0, 5));
    assert!((loud.loudness - quiet.loudness - 20.0).abs() < 0.2);
    for (a, b) in loud.curve.iter().zip(&quiet.curve) {
        assert!((a - b).abs() < 0.05, "{a} vs {b}");
    }
    for (a, b) in loud.regions.iter().zip(&quiet.regions) {
        assert!((a - b).abs() < 0.05);
    }
}

#[test]
fn more_sub_raises_the_sub_region() {
    let noise = pink(6.0, -20.0, 7);
    let with_sub = add(&noise, &both(&sine(RATE, 45.0, -14.0, 0.0, seconds(6.0))));
    let plain = summary_of(&noise);
    let heavy = summary_of(&with_sub);
    let sub = Region::ALL.iter().position(|&r| r == Region::Sub).unwrap();
    let high = Region::ALL.iter().position(|&r| r == Region::High).unwrap();
    assert!(heavy.regions[sub] > plain.regions[sub] + 6.0);
    assert!(heavy.regions[high] < plain.regions[high]);
}

#[test]
fn the_low_end_mono_reading() {
    let tone = sine(RATE, 50.0, -12.0, 0.0, seconds(4.0));
    let mono = summary_of(&both(&tone));
    assert!(mono.low_end_mono.unwrap() > 0.99);
    let inverted: Vec<f32> = tone.iter().map(|x| -x).collect();
    let opposite = summary_of(&stereo(&tone, &inverted));
    assert!(opposite.low_end_mono.unwrap() < -0.99);
    // Highs alone leave the low end silent.
    let highs = summary_of(&both(&sine(RATE, 5_000.0, -12.0, 0.0, seconds(4.0))));
    assert_eq!(highs.low_end_mono, None);
}

#[test]
fn a_steady_sub_sine_has_a_crest_of_3_db() {
    let summary = summary_of(&both(&sine(RATE, 40.0, -12.0, 0.0, seconds(6.0))));
    let crest = summary.low_end_crest[0].expect("the Sub has a crest");
    assert!((crest - 3.01).abs() < 0.5, "crest {crest:.2} dB");
}

#[test]
fn keys_are_found_from_chords() {
    // A minor (A2, A3, C4, E4) and C major (C3, C4, E4, G4).
    let chord = |notes: &[f64]| -> Vec<f32> {
        let mut audio = vec![0.0f32; seconds(6.0)];
        for &f in notes {
            for (a, s) in audio
                .iter_mut()
                .zip(sine(RATE, f, -18.0, 0.0, seconds(6.0)))
            {
                *a += s;
            }
        }
        both(&audio)
    };
    let a_minor = summary_of(&chord(&[110.0, 220.0, 261.63, 329.63]))
        .key
        .unwrap();
    assert_eq!(
        a_minor.key,
        Key {
            tonic: 9,
            minor: true
        },
        "{}",
        a_minor.key.name()
    );
    let c_major = summary_of(&chord(&[130.81, 261.63, 329.63, 392.0]))
        .key
        .unwrap();
    assert_eq!(
        c_major.key,
        Key {
            tonic: 0,
            minor: false
        },
        "{}",
        c_major.key.name()
    );
    assert!(c_major.confidence > 0.0);
    assert_eq!(detect_key(&[0.0; 12]), None);
}

#[test]
fn keys_name_and_measure_the_way_between_them() {
    let a = Key {
        tonic: 9,
        minor: true,
    };
    let f = Key {
        tonic: 5,
        minor: true,
    };
    assert_eq!(a.name(), "A minor");
    assert_eq!(a.semitones_to(f), -4);
    assert_eq!(f.semitones_to(a), 4);
    assert_eq!(
        Key {
            tonic: 0,
            minor: false
        }
        .semitones_to(Key {
            tonic: 6,
            minor: false
        }),
        6
    );
}

#[test]
fn shifting_moves_only_the_fine_low_end() {
    let grid = tonal_grid();
    let curve: Vec<f32> = (0..grid.len()).map(|i| i as f32).collect();
    let up = shift_low_end(&curve, 1);
    let fine = fine_points();
    // One semitone up: each fine point takes the value two points below.
    assert_eq!(up[10], 8.0);
    assert_eq!(up[0], 0.0); // held at the bottom end
    assert_eq!(up[fine], fine as f32); // above 300 Hz untouched
    let down = shift_low_end(&curve, -2);
    assert_eq!(down[10], 14.0);
    assert_eq!(down[fine - 1], (fine - 1) as f32); // held at the top of the fine part
}

#[test]
fn loud_parts_leave_out_a_quiet_intro() {
    let intro = pink(10.0, -40.0, 11);
    let drop = pink(10.0, -12.0, 13);
    let song: Vec<f32> = intro.iter().chain(&drop).copied().collect();
    let frames = frames_of(&song);
    let whole = summarise(&frames, Parts::Whole).unwrap();
    let loud = summarise(&frames, Parts::Loud).unwrap();
    assert!(
        loud.seconds < whole.seconds * 0.7,
        "{} vs {}",
        loud.seconds,
        whole.seconds
    );
    assert!(loud.loudness > whole.loudness + 2.0);
}

#[test]
fn silence_is_left_out() {
    let audio: Vec<f32> = pink(4.0, -20.0, 17)
        .into_iter()
        .chain(silence(seconds(6.0) * 2))
        .collect();
    let summary = summary_of(&audio);
    assert!(summary.seconds < 6.0, "counted {} s", summary.seconds);
}

#[test]
fn a_sliding_live_summary_forgets() {
    let mut live = LiveSummary::new(LongTerm::Seconds(3.0));
    for frame in frames_of(&both(&sine(RATE, 45.0, -12.0, 0.0, seconds(5.0)))) {
        live.push(frame);
    }
    for frame in frames_of(&pink(8.0, -20.0, 19)) {
        live.push(frame);
    }
    let summary = live.summary().unwrap();
    assert!((summary.seconds - 3.0).abs() < 0.01);
    let plain = summary_of(&pink(8.0, -20.0, 19));
    let sub = Region::ALL.iter().position(|&r| r == Region::Sub).unwrap();
    assert!((summary.regions[sub] - plain.regions[sub]).abs() < 2.0);
}

mod targets {
    use dasmeter_analysis::tonal_balance::{
        Band, COMMON_KEY, Key, KeyEstimate, MIN_RANGE_DB, SongShape, combine, fine_points,
        tonal_grid,
    };

    /// A song whose fine low end peaks two points (a semitone) per tonic step above C.
    fn song(tonic: u8, sure: bool) -> SongShape {
        let points = tonal_grid().len();
        let mut curve = vec![-20.0; points];
        let peak = 20 + 2 * usize::from(tonic);
        curve[peak] = 0.0;
        SongShape {
            curve,
            regions: [-10.0, -12.0, -15.0, -18.0, -25.0],
            key: Some(KeyEstimate {
                key: Key { tonic, minor: true },
                confidence: if sure { 0.6 } else { 0.0 },
            }),
            low_end_crest: [Some(6.0), Some(8.0)],
            low_end_mono: Some(0.95),
        }
    }

    #[test]
    fn songs_in_different_keys_line_up_before_averaging() {
        // Peaks on F, D and E: each the fewest semitones down to C, they
        // land on one point. (A would go three up, to the C an octave higher.)
        let combined = combine(&[song(5, true), song(2, true), song(4, true)]).unwrap();
        assert_eq!(combined.aligned, 3);
        assert_eq!(combined.key, COMMON_KEY);
        let top = combined
            .curve
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.average.total_cmp(&b.1.average))
            .unwrap();
        assert_eq!(top.0, 20, "the peaks met at point 20 (C)");
        assert!(top.1.average > -0.01);
    }

    #[test]
    fn unsure_keys_are_left_where_they_are() {
        let combined = combine(&[song(9, false), song(5, false)]).unwrap();
        assert_eq!(combined.aligned, 0);
        // Two separate peaks, each averaged with the other song's floor.
        assert!((combined.curve[38].average + 10.0).abs() < 0.01);
        assert!((combined.curve[30].average + 10.0).abs() < 0.01);
    }

    #[test]
    fn the_range_is_the_spread_but_never_narrower_than_the_minimum() {
        let one = combine(&[song(0, true)]).unwrap();
        assert_eq!(
            one.regions[0],
            Band {
                average: -10.0,
                range: MIN_RANGE_DB
            }
        );
        let mut loud = song(0, true);
        loud.regions[0] = 0.0;
        let mut quiet = song(0, true);
        quiet.regions[0] = -20.0;
        let two = combine(&[loud, quiet]).unwrap();
        assert_eq!(two.regions[0].average, -10.0);
        assert!((two.regions[0].range - 10.0).abs() < 1e-4);
    }

    #[test]
    fn a_target_moves_to_the_mix_key() {
        let combined = combine(&[song(0, true)]).unwrap();
        let in_e = combined.in_key(Key {
            tonic: 4,
            minor: false,
        });
        // C → E is four semitones up: the peak moves eight points.
        let top = in_e
            .curve
            .iter()
            .take(fine_points())
            .enumerate()
            .max_by(|a, b| a.1.average.total_cmp(&b.1.average))
            .unwrap()
            .0;
        assert_eq!(top, 28);
        assert_eq!(in_e.key.tonic, 4);
        assert_eq!(in_e.regions, combined.regions);
    }

    #[test]
    fn outside_tells_how_far_from_the_range() {
        let band = Band {
            average: -10.0,
            range: 2.0,
        };
        assert_eq!(band.outside(-11.0), 0.0);
        assert_eq!(band.outside(-6.0), 2.0);
        assert_eq!(band.outside(-15.0), -3.0);
        assert_eq!(combine(&[]), None);
    }
}
