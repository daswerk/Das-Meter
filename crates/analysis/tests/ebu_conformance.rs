//! EBU conformance (#45, ADR 0005): the Loudness Meter analyser against the
//! EBU loudness test set (Tech 3341 and 3342) at 48 kHz. Opt-in:
//!
//! ```text
//! cargo test -p dasmeter-analysis --features ebu-conformance --test ebu_conformance -- --nocapture
//! ```
//!
//! The files may not be redistributed, so they're never committed: download
//! the test set from <https://tech.ebu.ch/publications/ebu_loudness_test_set>
//! and unzip it into `ebu-test-set/` at the repository root (git-ignored), or
//! point `DASMETER_EBU_TEST_SET` at the folder. Without them the run reports
//! **skipped**, never passed; with `DASMETER_EBU_REQUIRE=1` (the release
//! checklist) a missing file fails it.
//!
//! The expected values are the ones Tech 3341 and 3342 give. Case 6 of 3341
//! is 5.0 and 5.1 surround, which the stereo Loudness Meter doesn't measure.
#![cfg(feature = "ebu-conformance")]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dasmeter_analysis::{LoudnessAnalyser, LoudnessSettings};

const RATE: u32 = 48_000;
const LUFS_TOLERANCE: f64 = 0.1;
const LRA_TOLERANCE: f64 = 1.0;
/// True peak may read 0.4 dB under or 0.2 dB over.
const TRUE_PEAK_UNDER: f64 = 0.4;
const TRUE_PEAK_OVER: f64 = 0.2;

/// What a case measures, and how.
#[derive(Clone, Copy)]
enum Check {
    Integrated(f64),
    Range(f64),
    TruePeak(f64),
    /// Momentary LUFS stays at this from 1 s on.
    MomentarySteady(f64),
    /// Short-term LUFS stays at this from 3 s on.
    ShortTermSteady(f64),
    /// The highest momentary LUFS.
    MomentaryMax(f64),
    /// The highest short-term LUFS.
    ShortTermMax(f64),
    /// The highest momentary LUFS so far at the end of each `seconds`-long
    /// segment: −38 LUFS after the first, a LU more after each.
    MomentaryStairs {
        seconds: f64,
    },
    /// The same for short-term LUFS.
    ShortTermStairs {
        seconds: f64,
    },
}

struct Case {
    name: String,
    /// The start of the file's name: the set's file names carry version
    /// suffixes and, for some, a doubled ".wav".
    file: String,
    checks: Vec<Check>,
}

fn case(name: &str, file: &str, checks: &[Check]) -> Case {
    Case {
        name: name.to_owned(),
        file: file.to_owned(),
        checks: checks.to_vec(),
    }
}

fn cases() -> Vec<Case> {
    use Check::*;
    let mut cases = vec![
        case(
            "3341-1",
            "seq-3341-1-",
            &[
                Integrated(-23.0),
                MomentarySteady(-23.0),
                ShortTermSteady(-23.0),
            ],
        ),
        case(
            "3341-2",
            "seq-3341-2-",
            &[
                Integrated(-33.0),
                MomentarySteady(-33.0),
                ShortTermSteady(-33.0),
            ],
        ),
        case("3341-3", "seq-3341-3-", &[Integrated(-23.0)]),
        case("3341-4", "seq-3341-4-", &[Integrated(-23.0)]),
        case("3341-5", "seq-3341-5-", &[Integrated(-23.0)]),
        case(
            "3341-7 / 3342-5",
            "seq-3341-7_seq-3342-5-",
            &[Integrated(-23.0), Range(5.0)],
        ),
        case(
            "3341-8 / 3342-6",
            "seq-3341-2011-8_seq-3342-6-",
            &[Integrated(-23.0), Range(15.0)],
        ),
        case("3341-9", "seq-3341-9-", &[ShortTermSteady(-23.0)]),
    ];
    for i in 1..=20 {
        cases.push(case(
            &format!("3341-10-{i}"),
            &format!("seq-3341-10-{i}-"),
            &[ShortTermMax(-23.0)],
        ));
    }
    cases.push(case(
        "3341-11",
        "seq-3341-11-",
        &[ShortTermStairs { seconds: 6.0 }],
    ));
    cases.push(case("3341-12", "seq-3341-12-", &[MomentarySteady(-23.0)]));
    for i in 1..=20 {
        cases.push(case(
            &format!("3341-13-{i}"),
            &format!("seq-3341-13-{i}-"),
            &[MomentaryMax(-23.0)],
        ));
    }
    cases.push(case(
        "3341-14",
        "seq-3341-14-",
        &[MomentaryStairs { seconds: 8.0 }],
    ));
    for (i, peak) in [
        (15, -6.0),
        (16, -6.0),
        (17, -6.0),
        (18, -6.0),
        (19, 3.0),
        (20, 0.0),
        (21, 0.0),
        (22, 0.0),
        (23, 0.0),
    ] {
        cases.push(case(
            &format!("3341-{i}"),
            &format!("seq-3341-{i}-"),
            &[TruePeak(peak)],
        ));
    }
    for (i, range) in [(1, 10.0), (2, 5.0), (3, 20.0), (4, 15.0)] {
        cases.push(case(
            &format!("3342-{i}"),
            &format!("seq-3342-{i}-"),
            &[Range(range)],
        ));
    }
    cases
}

/// A WAV file's sample rate, channel count and samples (interleaved, ±1).
/// Integer PCM of 16, 24 or 32 bits and 32-bit float, plain or extensible.
fn read_wav(path: &Path) -> Result<(u32, usize, Vec<f32>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
    let mut format = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let (id, size) = (&bytes[at..at + 4], u32_at(at + 4) as usize);
        let body = at + 8;
        let end = (body + size).min(bytes.len());
        if id == b"fmt " {
            let mut tag = u16_at(body);
            if tag == 0xFFFE {
                // WAVE_FORMAT_EXTENSIBLE: the real format leads the sub-format GUID.
                tag = u16_at(body + 24);
            }
            format = Some((tag, u16_at(body + 2), u32_at(body + 4), u16_at(body + 14)));
        } else if id == b"data" {
            let (tag, channels, rate, bits) = format.ok_or("data before fmt")?;
            let data = &bytes[body..end];
            let samples: Vec<f32> = match (tag, bits) {
                (1, 16) => data
                    .chunks_exact(2)
                    .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
                    .collect(),
                (1, 24) => data
                    .chunks_exact(3)
                    .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0)
                    .collect(),
                (1, 32) => data
                    .chunks_exact(4)
                    .map(|b| {
                        (f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])) / 2_147_483_648.0)
                            as f32
                    })
                    .collect(),
                (3, 32) => data
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect(),
                _ => return Err(format!("unsupported format {tag}, {bits} bits")),
            };
            return Ok((rate, usize::from(channels), samples));
        }
        // Chunks are padded to an even length.
        at = body + size + (size & 1);
    }
    Err("no audio data".into())
}

/// Every WAV file under `folder`, however deep (the set unzips into a folder of its own).
fn wav_files(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn find<'a>(files: &'a [PathBuf], start: &str) -> Option<&'a PathBuf> {
    files.iter().find(|path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(start))
    })
}

/// One expected value against the measured one.
struct Row {
    case: String,
    what: String,
    expected: f64,
    measured: f64,
    tolerance: String,
    pass: bool,
}

fn row(case: &str, what: String, expected: f64, measured: f64, tolerance: f64) -> Row {
    Row {
        case: case.to_owned(),
        what,
        expected,
        measured,
        tolerance: format!("±{tolerance}"),
        pass: (measured - expected).abs() <= tolerance,
    }
}

/// Runs the file through the analyser as the app does, a block at a time,
/// reading the meter after every 10 ms, and checks each of the case's values.
fn run(case: &Case, audio: &[f32]) -> Vec<Row> {
    let mut analyser = LoudnessAnalyser::new(RATE, LoudnessSettings::default());
    let block = RATE as usize / 100;
    let mut momentary = Vec::new();
    let mut short_term = Vec::new();
    for chunk in audio.chunks(2 * block) {
        analyser.process(chunk);
        let readings = analyser.readings();
        momentary.push(readings.momentary);
        short_term.push(readings.short_term);
    }
    let readings = analyser.readings();
    let blocks_in = |seconds: f64| (seconds * 100.0).round() as usize;
    let mut rows = Vec::new();
    for check in &case.checks {
        match *check {
            Check::Integrated(want) => rows.push(row(
                &case.name,
                "integrated (LUFS)".into(),
                want,
                readings.integrated,
                LUFS_TOLERANCE,
            )),
            Check::Range(want) => rows.push(row(
                &case.name,
                "LRA (LU)".into(),
                want,
                readings.range,
                LRA_TOLERANCE,
            )),
            Check::TruePeak(want) => {
                let got = readings.true_peak_max;
                rows.push(Row {
                    case: case.name.clone(),
                    what: "true peak (dBTP)".into(),
                    expected: want,
                    measured: got,
                    tolerance: format!("+{TRUE_PEAK_OVER}/−{TRUE_PEAK_UNDER}"),
                    pass: got >= want - TRUE_PEAK_UNDER && got <= want + TRUE_PEAK_OVER,
                });
            }
            Check::MomentarySteady(want) | Check::ShortTermSteady(want) => {
                let (values, from, what) = match check {
                    Check::MomentarySteady(_) => (&momentary, 1.0, "momentary from 1 s"),
                    _ => (&short_term, 3.0, "short-term from 3 s"),
                };
                // The reading furthest from the expected one.
                let worst = values[blocks_in(from).min(values.len())..]
                    .iter()
                    .copied()
                    .max_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()))
                    .unwrap_or(f64::NEG_INFINITY);
                rows.push(row(
                    &case.name,
                    format!("{what}, furthest (LUFS)"),
                    want,
                    worst,
                    LUFS_TOLERANCE,
                ));
            }
            Check::MomentaryMax(want) | Check::ShortTermMax(want) => {
                let (values, what) = match check {
                    Check::MomentaryMax(_) => (&momentary, "momentary max (LUFS)"),
                    _ => (&short_term, "short-term max (LUFS)"),
                };
                let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                rows.push(row(&case.name, what.into(), want, max, LUFS_TOLERANCE));
            }
            Check::MomentaryStairs { seconds } | Check::ShortTermStairs { seconds } => {
                let (values, what) = match check {
                    Check::MomentaryStairs { .. } => (&momentary, "momentary max"),
                    _ => (&short_term, "short-term max"),
                };
                let segment = blocks_in(seconds);
                let mut max = f64::NEG_INFINITY;
                for (i, value) in values.iter().enumerate() {
                    max = max.max(*value);
                    if (i + 1) % segment == 0 {
                        let k = (i + 1) / segment - 1;
                        rows.push(row(
                            &case.name,
                            format!("{what} by {:.0} s (LUFS)", (k + 1) as f64 * seconds),
                            -38.0 + k as f64,
                            max,
                            LUFS_TOLERANCE,
                        ));
                    }
                }
            }
        }
    }
    rows
}

fn folder() -> PathBuf {
    std::env::var_os("DASMETER_EBU_TEST_SET").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ebu-test-set"),
        PathBuf::from,
    )
}

fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.2}")
    } else {
        "–∞".into()
    }
}

#[test]
fn ebu_loudness_test_set() {
    let folder = folder();
    let files = wav_files(&folder);
    let require = std::env::var("DASMETER_EBU_REQUIRE").is_ok_and(|v| v == "1");
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();
    for case in cases() {
        let Some(path) = find(&files, &case.file) else {
            skipped.push(case.name);
            continue;
        };
        match read_wav(path) {
            Ok((RATE, 2, audio)) => rows.extend(run(&case, &audio)),
            Ok((rate, channels, _)) => errors.push(format!(
                "{}: {} is {rate} Hz, {channels} channels; expected 48 kHz stereo",
                case.name,
                path.display()
            )),
            Err(e) => errors.push(format!("{}: {}: {e}", case.name, path.display())),
        }
    }

    let failed = rows.iter().filter(|r| !r.pass).count();
    let outcome = if rows.is_empty() && errors.is_empty() {
        "skipped"
    } else if failed > 0 || !errors.is_empty() {
        "failed"
    } else if skipped.is_empty() {
        "passed"
    } else {
        "partly run"
    };
    let mut report = format!("## EBU conformance: {outcome}\n\n");
    if rows.is_empty() && errors.is_empty() {
        let _ = writeln!(
            report,
            "No EBU test files in `{}`, so nothing was checked. Download the set from \
             <https://tech.ebu.ch/publications/ebu_loudness_test_set> and unzip it there, \
             or set `DASMETER_EBU_TEST_SET`.",
            folder.display()
        );
    } else {
        let _ = writeln!(
            report,
            "| Case | Measures | Expected | Measured | Tolerance | |\n|---|---|---|---|---|---|"
        );
        for r in &rows {
            let _ = writeln!(
                report,
                "| {} | {} | {} | {} | {} | {} |",
                r.case,
                r.what,
                number(r.expected),
                number(r.measured),
                r.tolerance,
                if r.pass { "pass" } else { "**FAIL**" }
            );
        }
        let _ = writeln!(report, "\n{} checks, {failed} failed.", rows.len());
        if !skipped.is_empty() {
            let _ = writeln!(
                report,
                "\nSkipped (file not found): {}.",
                skipped.join(", ")
            );
        }
        for e in &errors {
            let _ = writeln!(report, "\nError: {e}");
        }
    }
    println!("{report}");
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)
        {
            let _ = writeln!(file, "{report}");
        }
        if outcome == "skipped" {
            println!(
                "::warning title=EBU conformance skipped::No EBU test files, so nothing was checked."
            );
        }
    }

    assert!(errors.is_empty(), "files that couldn't be read: {errors:?}");
    assert_eq!(
        failed, 0,
        "{failed} EBU conformance checks failed (see the report above)"
    );
    assert!(
        !require || skipped.is_empty(),
        "DASMETER_EBU_REQUIRE=1 but these cases' files are missing: {skipped:?}"
    );
}

/// The reader itself, on a file written here: always runs with the feature.
#[test]
fn the_wav_reader_reads_16_and_24_bit_pcm() {
    let dir = std::env::temp_dir().join(format!("dasmeter-ebu-wav-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp folder");
    let write = |name: &str, bits: u16, tag: u16, samples: &[i32]| {
        let bytes_per = usize::from(bits / 8);
        let mut data = Vec::new();
        for s in samples {
            data.extend_from_slice(&s.to_le_bytes()[..bytes_per]);
        }
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&RATE.to_le_bytes());
        fmt.extend_from_slice(&(RATE * 2 * u32::from(bits / 8)).to_le_bytes());
        fmt.extend_from_slice(&(2 * bits / 8).to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        if tag == 0xFFFE {
            fmt.extend_from_slice(&22u16.to_le_bytes());
            fmt.extend_from_slice(&bits.to_le_bytes());
            fmt.extend_from_slice(&3u32.to_le_bytes());
            fmt.extend_from_slice(&1u16.to_le_bytes());
            fmt.extend_from_slice(&[0; 14]);
        }
        let mut file = b"RIFF\0\0\0\0WAVE".to_vec();
        // An odd-sized chunk first, padded, as some writers add.
        file.extend_from_slice(b"LIST");
        file.extend_from_slice(&3u32.to_le_bytes());
        file.extend_from_slice(b"abc\0");
        file.extend_from_slice(b"fmt ");
        file.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        file.extend_from_slice(&fmt);
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        let path = dir.join(name);
        std::fs::write(&path, file).expect("write");
        path
    };
    let path = write("a.wav", 16, 1, &[16_384, -32_768]);
    let (rate, channels, samples) = read_wav(&path).expect("16-bit");
    assert_eq!((rate, channels), (RATE, 2));
    assert_eq!(samples, vec![0.5, -1.0]);
    let path = write("b.wav", 24, 0xFFFE, &[4_194_304, -8_388_608]);
    let (_, _, samples) = read_wav(&path).expect("24-bit extensible");
    assert_eq!(samples, vec![0.5, -1.0]);
    assert!(find(&wav_files(&dir), "b").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
