//! Audio files for the Tonal Balance Meter: decoded with Symphonia and
//! analysed into a summary far faster than real time, on a thread of their
//! own, for a Target's songs (Add files…, or files dropped on the Meter) or
//! a mix to compare (Compare file…).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use dasmeter_analysis::tonal_balance::{FrameAnalyser, Parts, Summary, summarise};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// The file extensions offered and looked for in dropped folders.
pub const EXTENSIONS: &[&str] = &[
    "wav", "wave", "aif", "aiff", "aifc", "flac", "mp3", "ogg", "oga", "m4a", "mp4", "aac", "caf",
];

/// The longest a file may be: a Target's song or a mix, not a DJ set.
const MAX_SECONDS: f64 = 2.0 * 3600.0;

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The audio files among `paths`, folders searched through, in name order.
pub fn audio_files_in(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = paths.to_vec();
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.filter_map(Result::ok).map(|e| e.path()));
            }
        } else if is_audio(&path) {
            found.push(path);
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The name for a Target made from `paths` (as picked or dropped): the
/// folder dropped, or the folder the first file is in.
pub fn target_name(paths: &[PathBuf]) -> String {
    let folder = paths.first().and_then(|first| {
        if first.is_dir() {
            first.file_name()
        } else {
            first.parent().and_then(Path::file_name)
        }
    });
    folder.map_or_else(|| "Target".to_owned(), |f| f.to_string_lossy().into_owned())
}

/// A file's name without its folder or extension: a song's name.
pub fn song_name(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "Song".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Decodes `path`, handing `each` its audio as interleaved stereo (mono
/// doubled, beyond two channels the first two) with its sample rate.
pub fn decode(path: &Path, mut each: impl FnMut(&[f32], u32)) -> Result<(), String> {
    decode_while(path, |audio, rate| {
        each(audio, rate);
        true
    })
}

/// As [`decode`], stopping early when `each` returns false.
pub fn decode_while(path: &Path, mut each: impl FnMut(&[f32], u32) -> bool) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| "not an audio file Das-Meter can read".to_owned())?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("no audio in it")?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("no audio in it")?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|_| "its audio format isn't supported".to_owned())?;
    let mut samples: Vec<f32> = Vec::new();
    let mut stereo: Vec<f32> = Vec::new();
    let mut frames = 0u64;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buffer = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            // A damaged packet is skipped, as players do.
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        let rate = buffer.spec().rate();
        let channels = buffer.spec().channels().count();
        if channels == 0 || rate == 0 {
            continue;
        }
        buffer.copy_to_vec_interleaved(&mut samples);
        stereo.clear();
        for frame in samples.chunks_exact(channels) {
            let (l, r) = (frame[0], frame[channels.min(2) - 1]);
            stereo.extend_from_slice(&[l, r]);
        }
        frames += (stereo.len() / 2) as u64;
        if frames as f64 > MAX_SECONDS * f64::from(rate) {
            return Err("longer than two hours".into());
        }
        if !each(&stereo, rate) {
            return Ok(());
        }
    }
    if frames == 0 {
        return Err("no audio in it".into());
    }
    Ok(())
}

/// Analyses a file as the Tonal Balance Meter would hear it: every frame
/// of it summed, over `parts`.
pub fn analyse(path: &Path, mono_cutoff: f32, parts: Parts) -> Result<Summary, String> {
    let mut analyser: Option<FrameAnalyser> = None;
    let mut frames = Vec::new();
    decode(path, |audio, rate| {
        let analyser = match &mut analyser {
            Some(a) if a.sample_rate() == rate => a,
            _ => analyser.insert(FrameAnalyser::new(rate, mono_cutoff)),
        };
        analyser.process(audio);
        frames.extend(analyser.take_frames());
    })?;
    summarise(&frames, parts).ok_or_else(|| "only silence".to_owned())
}

/// What a batch of files is for.
#[derive(Clone, Debug)]
pub enum Purpose {
    /// Songs for the Target called this.
    Target(String),
    /// A mix to compare (the first file only).
    Compare,
}

/// A batch of files for one Meter.
#[derive(Clone, Debug)]
pub struct Job {
    pub meter: usize,
    pub purpose: Purpose,
    pub files: Vec<PathBuf>,
    pub mono_cutoff: f32,
    pub parts: Parts,
}

/// What the background thread reports.
pub enum Report {
    /// A file is done: its name and its summary, or why it couldn't be read.
    File {
        meter: usize,
        purpose: Purpose,
        parts: Parts,
        name: String,
        /// The file, for listening to it later (A/B).
        path: PathBuf,
        result: Result<Box<Summary>, String>,
    },
    /// The batch is done.
    Done { meter: usize },
}

/// The background thread: batches in, reports out, waking the app for each.
pub struct Uploads {
    jobs: Sender<Job>,
    reports: Receiver<Report>,
}

impl Uploads {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Uploads {
        let (jobs, job_queue) = channel::<Job>();
        let (report, reports) = channel();
        std::thread::Builder::new()
            .name("audio files".into())
            .spawn(move || {
                for job in job_queue {
                    for path in &job.files {
                        let result = analyse(path, job.mono_cutoff, job.parts).map(Box::new);
                        let sent = report.send(Report::File {
                            meter: job.meter,
                            purpose: job.purpose.clone(),
                            parts: job.parts,
                            name: song_name(path),
                            path: path.clone(),
                            result,
                        });
                        wake();
                        if sent.is_err() {
                            return;
                        }
                    }
                    let _ = report.send(Report::Done { meter: job.meter });
                    wake();
                }
            })
            .expect("a thread for audio files");
        Uploads { jobs, reports }
    }

    pub fn add(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn reports(&self) -> impl Iterator<Item = Report> + '_ {
        self.reports.try_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit stereo WAV of a 55 Hz sine, `seconds` long.
    fn wav(path: &Path, seconds: f64) {
        let rate = 44_100u32;
        let frames = (seconds * f64::from(rate)) as u32;
        let mut data = Vec::new();
        for i in 0..frames {
            let t = f64::from(i) / f64::from(rate);
            let s = ((std::f64::consts::TAU * 55.0 * t).sin() * 0.5 * 32767.0) as i16;
            data.extend_from_slice(&s.to_le_bytes());
            data.extend_from_slice(&s.to_le_bytes());
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * 4).to_le_bytes());
        file.extend_from_slice(&4u16.to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        std::fs::write(path, file).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dasmeter-audio-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn a_wav_file_is_analysed_like_the_live_meter() {
        let path = scratch("sine.wav");
        wav(&path, 4.0);
        let summary = analyse(&path, 120.0, Parts::Whole).unwrap();
        assert!(summary.seconds >= 3.0, "{}", summary.seconds);
        let sub = summary.sub.expect("a Sub note");
        assert_eq!(sub.note.to_string(), "A1");
        assert!((sub.frequency - 55.0).abs() < 1.0);
        assert!(summary.low_end_mono.unwrap() > 0.99);
    }

    #[test]
    fn what_isnt_audio_is_refused_with_a_reason() {
        let path = scratch("notes.wav");
        std::fs::write(&path, b"not audio at all").unwrap();
        assert!(analyse(&path, 120.0, Parts::Loud).is_err());
        assert!(analyse(&scratch("missing.flac"), 120.0, Parts::Loud).is_err());
    }

    #[test]
    fn folders_are_searched_for_audio_files() {
        let dir = scratch("folder");
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        for name in ["b.wav", "a.FLAC", "cover.jpg", "inner/c.mp3"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        let found: Vec<String> = audio_files_in(std::slice::from_ref(&dir))
            .iter()
            .map(|p| {
                // "/" between folders on every system, to compare.
                let parts: Vec<String> = p
                    .strip_prefix(&dir)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                parts.join("/")
            })
            .collect();
        assert_eq!(found, ["a.FLAC", "b.wav", "inner/c.mp3"]);
    }
}
