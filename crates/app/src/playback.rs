//! A/B listening: a Tonal Balance Meter's reference song played on the
//! default output, looping, at the gain that matches it to the mix.
//!
//! The song runs on while the mix (A) is heard, faded to silence, so B picks
//! up where the song is, as a reference plugin does. A thread decodes the
//! file and resamples it to the output's rate into a ring; the output
//! callback only reads the ring and fades between gains, so switching never
//! clicks.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

/// How long the ring holds, in seconds of output.
const RING_SECONDS: f64 = 0.5;
/// How long a fade between gains takes, in seconds.
const FADE_SECONDS: f32 = 0.01;
/// How long the decoder waits for room in the ring.
const WAIT: Duration = Duration::from_millis(5);

/// One song playing for A/B.
pub struct Player {
    file: String,
    /// The gain the output fades to, as `f32` bits: 0 while A is heard.
    gain: Arc<AtomicU32>,
    /// Frames of the song the output has taken (heard or faded out).
    #[cfg(test)]
    played: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    decoder: Option<JoinHandle<()>>,
    _stream: Stream,
}

impl Player {
    /// Starts `file` on the default output, silent until [`Player::set_gain`].
    pub fn start(file: &str) -> Result<Player, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device is available")?;
        let format = device
            .default_output_config()
            .map_err(|e| format!("The output device has no usable format: {e}"))?;
        let (rate, channels) = (format.sample_rate(), usize::from(format.channels().max(1)));
        let capacity = ((f64::from(rate) * RING_SECONDS) as usize * 2).max(2);
        let (producer, consumer) = RingBuffer::<f32>::new(capacity);
        let gain = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let played = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let stream = open(
            &device,
            (rate, format.channels(), channels),
            consumer,
            &gain,
            &played,
        )?;
        let decoder = {
            let (path, stop) = (PathBuf::from(file), stop.clone());
            thread::Builder::new()
                .name("A/B playback".into())
                .spawn(move || decode(&path, rate, producer, &stop))
                .map_err(|e| e.to_string())?
        };
        Ok(Player {
            file: file.to_owned(),
            gain,
            #[cfg(test)]
            played,
            stop,
            decoder: Some(decoder),
            _stream: stream,
        })
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    /// How many frames of the song the output has taken so far.
    #[cfg(test)]
    pub fn played(&self) -> u64 {
        self.played.load(Relaxed)
    }

    /// Fades to `gain` (linear): the matched gain for B, 0 for A.
    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.max(0.0).to_bits(), Relaxed);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(decoder) = self.decoder.take() {
            let _ = decoder.join();
        }
    }
}

/// `format`: the output's rate, its channel count, and that as a `usize`.
fn open(
    device: &cpal::Device,
    (rate, channel_count, channels): (u32, u16, usize),
    mut ring: Consumer<f32>,
    gain: &Arc<AtomicU32>,
    played: &Arc<AtomicU64>,
) -> Result<Stream, String> {
    let config = StreamConfig {
        channels: channel_count,
        sample_rate: rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let (target, played) = (gain.clone(), played.clone());
    let step = 1.0 / (FADE_SECONDS * rate as f32);
    let mut now = 0.0f32;
    let stream = device
        .build_output_stream::<f32, _, _>(
            config,
            move |data: &mut [f32], _| {
                let to = f32::from_bits(target.load(Relaxed));
                let mut taken = 0;
                for frame in data.chunks_exact_mut(channels) {
                    // Short of audio (the decoder lags): silence, not a stall.
                    let (left, right) = match (ring.pop(), ring.pop()) {
                        (Ok(l), Ok(r)) => {
                            taken += 1;
                            (l, r)
                        }
                        _ => (0.0, 0.0),
                    };
                    now = if now < to {
                        (now + step * to.max(1.0)).min(to)
                    } else {
                        (now - step * now.max(1.0)).max(to)
                    };
                    // Turned up to match a louder mix, it never goes past full scale.
                    frame[0] = (left * now).clamp(-1.0, 1.0);
                    if let Some(second) = frame.get_mut(1) {
                        *second = (right * now).clamp(-1.0, 1.0);
                    }
                    for rest in frame.iter_mut().skip(2) {
                        *rest = 0.0;
                    }
                }
                played.fetch_add(taken, Relaxed);
            },
            |_: cpal::Error| {},
            None,
        )
        .map_err(|e| format!("Couldn't play the song: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("Couldn't play the song: {e}"))?;
    Ok(stream)
}

/// Decodes `path` over and over into `ring` at `rate`, until `stop`.
fn decode(path: &std::path::Path, rate: u32, mut ring: Producer<f32>, stop: &AtomicBool) {
    let mut out = Vec::new();
    while !stop.load(Relaxed) {
        let mut resampler: Option<Resampler> = None;
        let played = crate::audio_files::decode_while(path, |audio, from| {
            let resampler = match &mut resampler {
                Some(r) if r.from == from => r,
                _ => resampler.insert(Resampler::new(from, rate)),
            };
            out.clear();
            resampler.process(audio, &mut out);
            push(&mut ring, &out, stop)
        });
        // A file that can't be read (moved, deleted) stays silent.
        if played.is_err() {
            break;
        }
    }
}

/// Pushes all of `samples`, waiting for room; false once `stop` is set.
fn push(ring: &mut Producer<f32>, samples: &[f32], stop: &AtomicBool) -> bool {
    let mut at = 0;
    while at < samples.len() {
        if stop.load(Relaxed) {
            return false;
        }
        // Whole frames only, so left and right never swap.
        let room = ring.slots().min(samples.len() - at) & !1;
        if room == 0 {
            thread::sleep(WAIT);
            continue;
        }
        if let Ok(chunk) = ring.write_chunk_uninit(room) {
            chunk.fill_from_iter(samples[at..at + room].iter().copied());
        }
        at += room;
    }
    true
}

/// Stereo from one rate to another, by 4-point Hermite interpolation:
/// clean enough to listen to a reference by, and cheap.
struct Resampler {
    from: u32,
    /// Input frames per output frame.
    step: f64,
    /// Where the next output frame falls, in frames from `held[0]`.
    position: f64,
    /// The input frames not yet passed by.
    held: Vec<[f32; 2]>,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Resampler {
        Resampler {
            from,
            step: f64::from(from) / f64::from(to.max(1)),
            position: 1.0,
            // One frame before the first, so it has neighbours on both sides.
            held: vec![[0.0; 2]],
        }
    }

    /// Takes interleaved stereo `input`, adds what it can make to `out`.
    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.held.extend(input.as_chunks::<2>().0.iter().copied());
        if self.step == 1.0 {
            out.extend(self.held.drain(1..).flatten());
            return;
        }
        while self.position + 2.0 < self.held.len() as f64 {
            let i = self.position.floor() as usize;
            let t = (self.position - i as f64) as f32;
            for channel in 0..2 {
                let p = |k: usize| self.held[k][channel];
                out.push(hermite([p(i - 1), p(i), p(i + 1), p(i + 2)], t));
            }
            self.position += self.step;
        }
        // Keep the frame before the next position and everything after it.
        let keep_from = (self.position.floor() as usize).saturating_sub(1);
        self.held.drain(..keep_from);
        self.position -= keep_from as f64;
    }
}

fn hermite([a, b, c, d]: [f32; 4], t: f32) -> f32 {
    let c0 = b;
    let c1 = 0.5 * (c - a);
    let c2 = a - 2.5 * b + 2.0 * c - 0.5 * d;
    let c3 = 0.5 * (d - a) + 1.5 * (b - c);
    ((c3 * t + c2) * t + c1) * t + c0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let x = (std::f32::consts::TAU * hz * n as f32 / rate as f32).sin();
                [x, x]
            })
            .collect()
    }

    #[test]
    fn resampling_keeps_the_pitch_and_the_length() {
        let input = sine(44_100, 1_000.0, 44_100);
        let mut resampler = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for block in input.chunks(1_024) {
            resampler.process(block, &mut out);
        }
        let frames = out.len() / 2;
        assert!((47_990..=48_000).contains(&frames), "{frames} frames");
        // 1 kHz at 48 kHz: a zero crossing upward every 48 frames.
        let rising = out
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((998..=1_001).contains(&rising), "{rising} cycles");
        // And no level lost to the interpolation.
        let peak = out.iter().copied().fold(0.0f32, f32::max);
        assert!(peak > 0.99 && peak < 1.01, "{peak}");
    }

    #[test]
    fn the_same_rate_passes_through() {
        let input = sine(48_000, 440.0, 4_800);
        let mut resampler = Resampler::new(48_000, 48_000);
        let mut out = Vec::new();
        resampler.process(&input, &mut out);
        assert_eq!(out, input);
    }

    /// A WAV file of a 1 kHz sine, 16-bit stereo at `rate`, `seconds` long.
    fn wav(rate: u32, seconds: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as u32;
        let data = frames * 4;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data.to_le_bytes());
        for n in 0..frames {
            let x = (std::f32::consts::TAU * 1_000.0 * n as f32 / rate as f32).sin();
            let sample = ((x * 0.25) * f32::from(i16::MAX)) as i16;
            out.extend_from_slice(&sample.to_le_bytes());
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }

    /// Needs an output device: run by hand (`cargo test -- --ignored`).
    /// It plays at gain 0, so nothing is heard.
    #[test]
    #[ignore = "needs an audio output device"]
    fn a_song_plays_on_the_output_and_loops() {
        let path = std::env::temp_dir().join("das-meter-ab-test.wav");
        std::fs::write(&path, wav(44_100, 0.3)).unwrap();
        let player = Player::start(path.to_str().unwrap()).expect("an output to play on");
        std::thread::sleep(Duration::from_millis(1_000));
        let played = player.played();
        drop(player);
        let _ = std::fs::remove_file(&path);
        // A second of output from a 0.3 s file: it looped.
        assert!(played > 30_000, "{played} frames played");
    }
}
