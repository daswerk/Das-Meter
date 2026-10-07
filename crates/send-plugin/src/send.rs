//! The audio thread's part: pass the audio through and hand it to the transport.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use dasmeter_transport::{AudioWriter, Timing};

/// One block of the track's audio.
pub enum Block<'a> {
    Stereo(&'a [f32], &'a [f32]),
    Mono(&'a [f32]),
}

/// Sends audio from the audio thread. Makes no allocations, takes no locks and
/// makes no syscalls.
pub struct AudioSend {
    writer: Option<AudioWriter>,
    mono: Arc<AtomicBool>,
}

impl AudioSend {
    /// `writer` is `None` when no slot could be claimed; audio then only passes
    /// through. `mono` tells the main thread the track's channel layout.
    pub fn new(writer: Option<AudioWriter>, mono: Arc<AtomicBool>) -> AudioSend {
        AudioSend { writer, mono }
    }

    /// Hands one block to the transport, which copies it only while the app
    /// listens, with where the DAW is at its first frame (`None` if the host
    /// didn't say). A mono block goes out on both channels.
    pub fn send(&mut self, block: Block<'_>, timing: Option<&Timing>) {
        let (left, right, mono) = match block {
            Block::Stereo(left, right) => (left, right, false),
            Block::Mono(mono) => (mono, mono, true),
        };
        if self.mono.load(Relaxed) != mono {
            self.mono.store(mono, Relaxed);
        }
        if let Some(writer) = &mut self.writer {
            writer.set_timing(timing);
            writer.push(left, right);
        }
    }
}

/// Copies `input` to `output`, as far as both go. Stereo passes through unchanged.
pub fn pass_through(input: &[f32], output: &mut [f32]) {
    let n = input.len().min(output.len());
    output[..n].copy_from_slice(&input[..n]);
}

/// The host's transport as the app reads it: `None` unless the host gave a
/// tempo and a song position in beats. A missing time signature counts as 4/4.
pub fn timing(
    tempo: Option<f64>,
    beats: Option<f64>,
    bar_start: f64,
    signature: Option<(u16, u16)>,
    playing: bool,
) -> Option<Timing> {
    let (tempo, beats) = (tempo?, beats?);
    Some(Timing {
        tempo,
        beats,
        bar_start: if bar_start.is_finite() && bar_start <= beats {
            bar_start
        } else {
            beats
        },
        signature: signature.unwrap_or((4, 4)),
        playing,
    })
}
