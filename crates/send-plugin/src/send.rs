//! The audio thread's part: pass the audio through and hand it to the transport.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use dasmeter_transport::AudioWriter;

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
    /// listens. A mono block goes out on both channels.
    pub fn send(&mut self, block: Block<'_>) {
        let (left, right, mono) = match block {
            Block::Stereo(left, right) => (left, right, false),
            Block::Mono(mono) => (mono, mono, true),
        };
        if self.mono.load(Relaxed) != mono {
            self.mono.store(mono, Relaxed);
        }
        if let Some(writer) = &mut self.writer {
            writer.push(left, right);
        }
    }
}

/// Copies `input` to `output`, as far as both go. Stereo passes through unchanged.
pub fn pass_through(input: &[f32], output: &mut [f32]) {
    let n = input.len().min(output.len());
    output[..n].copy_from_slice(&input[..n]);
}
