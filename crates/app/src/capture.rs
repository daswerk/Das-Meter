//! System Capture through cpal: the default output device, at its own rate, as stereo.
//!
//! On macOS, cpal records an output device through a Core Audio process tap on
//! a private aggregate device (ADR 0002). A tap follows one device, so a
//! watcher thread polls the default output and its rate, and reopens the
//! stream when either changes.
//!
//! The audio callback only copies frames into a lock-free ring. It wakes the
//! event loop for audible blocks, and for silent ones only until the app core
//! reports that the Meters have settled (or the ring is half full), so silence
//! costs almost nothing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, Stream, StreamConfig};
use rtrb::{Consumer, RingBuffer};

/// How often the watcher checks the default output device and its rate.
const POLL: Duration = Duration::from_millis(250);
/// How long the ring holds: plenty for a main thread that stalls (window drags, resizes).
const RING_SECONDS: usize = 1;

/// From the capture thread to the main thread.
pub enum CaptureMessage {
    /// Audio now comes at `sample_rate`, as interleaved stereo frames in `audio`.
    /// Any previous ring is finished; drain it before switching.
    Started {
        sample_rate: u32,
        audio: Consumer<f32>,
    },
    /// Capture couldn't start. It keeps retrying.
    Failed(String),
}

/// Wakes the event loop at most once until the main thread has caught up.
pub struct Waker {
    pending: AtomicBool,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Waker {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Arc<Waker> {
        Arc::new(Waker {
            pending: AtomicBool::new(false),
            wake: Box::new(wake),
        })
    }

    pub fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }

    /// Called by the main thread when it starts handling a wake-up.
    pub fn clear(&self) {
        self.pending.store(false, Ordering::Release);
    }
}

/// Runs System Capture on its own thread until dropped.
pub struct SystemCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SystemCapture {
    /// `settled` is set by the main thread while the app core has nothing left to draw.
    pub fn start(
        waker: Arc<Waker>,
        settled: Arc<AtomicBool>,
    ) -> (SystemCapture, mpsc::Receiver<CaptureMessage>) {
        let (messages, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            thread::Builder::new()
                .name("system-capture".into())
                .spawn(move || watch(&stop, &messages, &waker, &settled))
                .expect("spawn the System Capture thread")
        };
        (
            SystemCapture {
                stop,
                thread: Some(thread),
            },
            receiver,
        )
    }
}

impl Drop for SystemCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The output device and format a stream was opened for.
#[derive(PartialEq, Eq, Debug)]
struct Output {
    device: String,
    sample_rate: u32,
    channels: u16,
}

fn current_output(host: &cpal::Host) -> Result<(cpal::Device, Output), String> {
    let device = host
        .default_output_device()
        .ok_or("No output device is available")?;
    let id = device.id().map_err(|e| e.to_string())?;
    let config = device
        .default_output_config()
        .map_err(|e| format!("The output device has no usable format: {e}"))?;
    Ok((
        device,
        Output {
            device: id.to_string(),
            sample_rate: config.sample_rate(),
            channels: config.channels(),
        },
    ))
}

fn watch(
    stop: &AtomicBool,
    messages: &mpsc::Sender<CaptureMessage>,
    waker: &Arc<Waker>,
    settled: &Arc<AtomicBool>,
) {
    let host = cpal::default_host();
    let mut last_failure = None;
    while !stop.load(Ordering::Acquire) {
        let opened = current_output(&host).and_then(|(device, output)| {
            open(&device, &output, waker, settled)
                .map(|(stream, audio, broken)| (output, stream, audio, broken))
        });
        match opened {
            Ok((output, stream, audio, broken)) => {
                last_failure = None;
                if messages
                    .send(CaptureMessage::Started {
                        sample_rate: output.sample_rate,
                        audio,
                    })
                    .is_err()
                {
                    return;
                }
                waker.wake();
                // Keep the stream until the output changes or the stream breaks.
                while !stop.load(Ordering::Acquire) && !broken.load(Ordering::Acquire) {
                    thread::sleep(POLL);
                    match current_output(&host) {
                        Ok((_, now)) if now == output => {}
                        _ => break,
                    }
                }
                drop(stream);
            }
            Err(reason) => {
                if last_failure.as_ref() != Some(&reason) {
                    last_failure = Some(reason.clone());
                    if messages.send(CaptureMessage::Failed(reason)).is_err() {
                        return;
                    }
                    waker.wake();
                }
                thread::sleep(POLL);
            }
        }
    }
}

type Opened = (Stream, Consumer<f32>, Arc<AtomicBool>);

fn open(
    device: &cpal::Device,
    output: &Output,
    waker: &Arc<Waker>,
    settled: &Arc<AtomicBool>,
) -> Result<Opened, String> {
    let channels = usize::from(output.channels.max(1));
    let capacity = output.sample_rate as usize * 2 * RING_SECONDS;
    let (mut producer, consumer) = RingBuffer::<f32>::new(capacity);
    let broken = Arc::new(AtomicBool::new(false));

    let config = StreamConfig {
        channels: output.channels,
        sample_rate: output.sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let data_waker = waker.clone();
    let settled = settled.clone();
    let error_broken = broken.clone();
    let error_waker = waker.clone();
    // Recording an output device makes cpal record what it plays (a process tap on macOS).
    let stream = device
        .build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], _| {
                let frames = data.len() / channels;
                // Downmix to stereo: the first two channels (front left and right); mono plays on both.
                if let Ok(chunk) = producer.write_chunk_uninit(frames * 2) {
                    chunk.fill_from_iter(data.chunks_exact(channels).flat_map(|frame| {
                        let left = frame[0];
                        [left, frame.get(1).copied().unwrap_or(left)]
                    }));
                }
                // If the ring is full the main thread has stalled for a second; the block is dropped.
                let silent = data.iter().all(|&x| x == 0.0);
                if !silent || !settled.load(Ordering::Acquire) || producer.slots() < capacity / 2 {
                    data_waker.wake();
                }
            },
            move |error: cpal::Error| match error.kind() {
                ErrorKind::Xrun | ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
                _ => {
                    error_broken.store(true, Ordering::Release);
                    error_waker.wake();
                }
            },
            None,
        )
        .map_err(|e| format!("System Capture couldn't start: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("System Capture couldn't start: {e}"))?;
    Ok((stream, consumer, broken))
}
