//! Hidden `--benchmark [busy|silent|both] [--seconds N] [--runs N]`: runs the
//! performance budget's scenes in a real window and reports the numbers the
//! budget is checked against.
//!
//! - **Busy**: Window mode at 1920 × 1080 logical with the four default
//!   Meters, fed the generated 48 kHz test signal in real time.
//! - **Silent**: the same, fed silence: the app should settle and sleep.
//!
//! Each run warms up for 5 s and measures for 60 s; the best of 3 counts. It
//! reports CPU (% of one core) per second as average, p95 and max; the time to
//! lay out and submit a frame; the share of late frames (more than 16.7 ms
//! apart while drawing); peak memory; and, from the signal's clicks, how long
//! a click takes to show on screen. GPU load isn't readable without admin
//! rights: see Activity Monitor ▸ Window ▸ GPU History while it runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dasmeter_analysis::test_signal::TestSignal;
use dasmeter_core::{AppCore, Decision, Event, LayoutMode, MeterState, MeterView, WindowKey};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::gpu::{Gpu, WindowSurface};
use crate::painter::Painter;

const RATE: u32 = 48_000;
/// Frames fed at once, as a capture callback would deliver them.
const BLOCK: usize = 256;
const WARM_UP: Duration = Duration::from_secs(5);
/// A frame later than this after the last one is late (60 fps with 1 ms slack).
const LATE: Duration = Duration::from_micros(17_700);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Busy,
    Silent,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Busy => "Busy",
            Kind::Silent => "Silent",
        }
    }
}

/// One run's numbers.
#[derive(Clone, Debug, Default)]
struct Result {
    /// CPU per second of the measurement, in % of one core.
    cpu: Vec<f64>,
    /// Time to lay out, draw and present each frame.
    frame_work: Vec<Duration>,
    frames: usize,
    late: usize,
    /// Click to the first frame showing it.
    latency: Vec<Duration>,
    peak_memory: u64,
}

fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let i = ((values.len() - 1) as f64 * p).round() as usize;
    values[i]
}

fn stats(values: &[f64]) -> (f64, f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted = values.to_vec();
    let average = values.iter().sum::<f64>() / values.len() as f64;
    let p95 = percentile(&mut sorted, 0.95);
    let max = sorted.last().copied().unwrap_or(0.0);
    (average, p95, max)
}

impl Result {
    fn average_cpu(&self) -> f64 {
        stats(&self.cpu).0
    }

    fn report(&self, kind: Kind) -> String {
        let (cpu, cpu95, cpu_max) = stats(&self.cpu);
        let ms: Vec<f64> = self
            .frame_work
            .iter()
            .map(|d| d.as_secs_f64() * 1e3)
            .collect();
        let (frame, frame95, frame_max) = stats(&ms);
        let lat: Vec<f64> = self.latency.iter().map(|d| d.as_secs_f64() * 1e3).collect();
        let (latency, latency95, latency_max) = stats(&lat);
        let late = if self.frames > 0 {
            100.0 * self.late as f64 / self.frames as f64
        } else {
            0.0
        };
        let mut out = format!(
            "{} scene\n  CPU (% of one core)   avg {cpu:5.1}   p95 {cpu95:5.1}   max {cpu_max:5.1}\n  \
             frames drawn          {} ({:.1} fps)\n  \
             frame work (ms)       avg {frame:5.2}   p95 {frame95:5.2}   max {frame_max:5.2}\n  \
             late frames (>16.7ms) {late:.2} %\n  \
             peak memory           {:.0} MB\n  \
             GPU                   not measured (Activity Monitor ▸ Window ▸ GPU History)\n",
            kind.name(),
            self.frames,
            self.frames as f64 / self.cpu.len().max(1) as f64,
            self.peak_memory as f64 / 1_048_576.0,
        );
        if kind == Kind::Busy {
            out += &format!(
                "  click → screen (ms)   avg {latency:5.1}   p95 {latency95:5.1}   max {latency_max:5.1}   ({} clicks; {:.1} frames at 60 fps)\n",
                self.latency.len(),
                latency / 16.667,
            );
        }
        out
    }
}

/// The process's CPU time so far, and its peak resident memory in bytes.
#[cfg(unix)]
fn usage() -> (Duration, u64) {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage fills the struct it's given for this process.
    let usage = unsafe {
        libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr());
        usage.assume_init()
    };
    let time = |t: libc::timeval| {
        Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64)
    };
    // macOS reports the peak in bytes, Linux in kilobytes.
    let peak = if cfg!(target_os = "macos") {
        usage.ru_maxrss as u64
    } else {
        usage.ru_maxrss as u64 * 1024
    };
    (time(usage.ru_utime) + time(usage.ru_stime), peak)
}

#[cfg(not(unix))]
fn usage() -> (Duration, u64) {
    (Duration::ZERO, 0)
}

struct Bench {
    queue: Vec<Kind>,
    runs: usize,
    seconds: Duration,
    signal: TestSignal,
    results: Vec<(Kind, Result)>,
    run: Option<Run>,
    window: Option<(Arc<Window>, WindowSurface, Painter)>,
}

/// The run in progress.
struct Run {
    kind: Kind,
    core: AppCore,
    start: Instant,
    fed: usize,
    measuring: bool,
    result: Result,
    /// CPU time at the start of the current second, and when it started.
    second: (Duration, Instant),
    last_present: Option<Instant>,
    /// When clicks were fed, waiting to show.
    clicks: Vec<Instant>,
    /// The Loudness Meter's peak showed a click on the last frame.
    peak_up: bool,
}

impl Run {
    fn new(kind: Kind) -> Run {
        let mut core = AppCore::new();
        core.handle(Event::SetMode(LayoutMode::Window), Duration::ZERO);
        core.handle(Event::CaptureStarted { sample_rate: RATE }, Duration::ZERO);
        Run {
            kind,
            core,
            start: Instant::now(),
            fed: 0,
            measuring: false,
            result: Result::default(),
            second: (usage().0, Instant::now()),
            last_present: None,
            clicks: Vec::new(),
            peak_up: false,
        }
    }
}

impl Bench {
    /// Feeds the signal up to now, in real time.
    fn feed(&mut self) -> Duration {
        let Some(run) = &mut self.run else {
            return Duration::ZERO;
        };
        let now = run.start.elapsed();
        let due = (now.as_secs_f64() * f64::from(RATE)) as usize;
        let total = self.signal.frames.len() / 2;
        let silence = [0.0f32; 2 * BLOCK];
        while run.fed + BLOCK <= due {
            let at = run.fed % total;
            let end = (at + BLOCK).min(total);
            let block = match run.kind {
                Kind::Busy => &self.signal.frames[2 * at..2 * end],
                Kind::Silent => &silence[..2 * (end - at)],
            };
            if run.kind == Kind::Busy && run.measuring {
                let clicks = self
                    .signal
                    .clicks
                    .iter()
                    .filter(|&&c| (at..end).contains(&c));
                for _ in clicks {
                    run.clicks.push(Instant::now());
                }
            }
            run.core.handle(Event::Audio(block), now);
            run.fed += end - at;
        }
        // The next block is due then.
        Duration::from_secs_f64((run.fed + BLOCK) as f64 / f64::from(RATE))
    }

    /// Whether the next block to feed has sound in it.
    fn next_block_audible(&self) -> bool {
        let Some(run) = &self.run else { return false };
        if run.kind == Kind::Silent {
            return false;
        }
        let total = self.signal.frames.len() / 2;
        let at = run.fed % total;
        let end = (at + BLOCK).min(total);
        self.signal.frames[2 * at..2 * end]
            .iter()
            .any(|&x| x != 0.0)
    }

    fn draw(&mut self) {
        let (Some(run), Some((window, surface, painter))) = (&mut self.run, &mut self.window)
        else {
            return;
        };
        let started = Instant::now();
        let Some(frame) = surface.frame(&painter.gpu) else {
            window.request_redraw();
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(
            run.core.scene(),
            Some(WindowKey::Main),
            window.scale_factor() as f32,
            &view,
            None,
        );
        window.pre_present_notify();
        painter.gpu.queue.present(frame);
        let presented = Instant::now();
        if run.measuring {
            run.result.frames += 1;
            run.result.frame_work.push(presented - started);
            if run.last_present.is_some_and(|last| presented - last > LATE) {
                run.result.late += 1;
            }
            // A click shows when the Loudness Meter's peak jumps up.
            let peak = run.core.scene().and_then(|scene| {
                scene.windows[0].meters.iter().find_map(|m| match &m.state {
                    MeterState::Live(MeterView::Loudness { display, .. }) => display.left.peak.db(),
                    _ => None,
                })
            });
            let up = peak.is_some_and(|db| db > -30.0);
            if up && !run.peak_up && !run.clicks.is_empty() {
                let fed = run.clicks.remove(0);
                run.result.latency.push(presented - fed);
            }
            run.peak_up = up;
        }
        run.last_present = Some(presented);
    }

    /// Starts measuring after the warm-up, samples CPU each second, and ends
    /// the run when it's done. Returns false when every run has finished.
    fn tick(&mut self) -> bool {
        let Some(run) = &mut self.run else {
            return false;
        };
        let elapsed = run.start.elapsed();
        if !run.measuring && elapsed >= WARM_UP {
            run.measuring = true;
            run.second = (usage().0, Instant::now());
            run.last_present = None;
        }
        if run.measuring && run.second.1.elapsed() >= Duration::from_secs(1) {
            let (cpu, _) = usage();
            let wall = run.second.1.elapsed();
            run.result
                .cpu
                .push(100.0 * (cpu - run.second.0).as_secs_f64() / wall.as_secs_f64());
            run.second = (cpu, Instant::now());
        }
        if elapsed < WARM_UP + self.seconds {
            return true;
        }
        let mut run = self.run.take().expect("a run");
        run.result.peak_memory = usage().1;
        eprintln!(
            "{} run {} of {}: CPU avg {:.1} %",
            run.kind.name(),
            self.results.iter().filter(|(k, _)| *k == run.kind).count() + 1,
            self.runs,
            run.result.average_cpu()
        );
        self.results.push((run.kind, run.result));
        match self.queue.pop() {
            Some(kind) => {
                self.run = Some(Run::new(kind));
                true
            }
            None => false,
        }
    }

    fn print(&self) {
        println!(
            "Das-Meter benchmark ({} s measured after {} s warm-up, best of {})\n",
            self.seconds.as_secs(),
            WARM_UP.as_secs(),
            self.runs
        );
        for kind in [Kind::Busy, Kind::Silent] {
            let best = self
                .results
                .iter()
                .filter(|(k, _)| *k == kind)
                .min_by(|a, b| a.1.average_cpu().total_cmp(&b.1.average_cpu()));
            if let Some((_, result)) = best {
                println!("{}", result.report(kind));
            }
        }
    }
}

impl ApplicationHandler for Bench {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Das-Meter benchmark")
            .with_inner_size(LogicalSize::new(1920.0, 1080.0));
        let window = Arc::new(event_loop.create_window(attributes).expect("a window"));
        let (gpu, surface) =
            pollster::block_on(Gpu::for_window(window.clone())).expect("a GPU to draw with");
        let painter = Painter::new(gpu);
        self.window = Some((window, surface, painter));
        let first = self.queue.pop().expect("a scene to run");
        self.run = Some(Run::new(first));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some((window, surface, painter)) = &mut self.window {
                    surface.resize(&mut painter.gpu, size.width, size.height);
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.tick() {
            self.print();
            event_loop.exit();
            return;
        }
        let next_block = self.feed();
        let audible = self.next_block_audible();
        let Some(run) = &mut self.run else { return };
        let now = run.start.elapsed();
        let wake = match run.core.decide(now) {
            Decision::Draw => {
                if let Some((window, ..)) = &self.window {
                    window.request_redraw();
                }
                next_block
            }
            // Nothing to draw: like capture, wake for the next audible block,
            // but for silence only when its ring would be half full.
            Decision::Sleep { until: None } if !audible => now + Duration::from_millis(500),
            Decision::Sleep { until: None } => next_block,
            Decision::Sleep { until: Some(until) } => until.min(next_block),
        };
        // Wake at least each second for the CPU samples.
        let wake = wake.min(now + Duration::from_secs(1));
        event_loop.set_control_flow(ControlFlow::WaitUntil(run.start + wake));
    }
}

/// Runs the benchmark with the command line's options.
pub fn run(args: &[String]) {
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let runs = option("--runs").unwrap_or(3).max(1) as usize;
    let seconds = Duration::from_secs(option("--seconds").unwrap_or(60).max(1));
    let kinds: &[Kind] = match args.first().map(String::as_str) {
        Some("busy") => &[Kind::Busy],
        Some("silent") => &[Kind::Silent],
        _ => &[Kind::Busy, Kind::Silent],
    };
    // Popped from the end: Busy's runs first.
    let mut queue = Vec::new();
    for &kind in kinds.iter().rev() {
        queue.extend(std::iter::repeat_n(kind, runs));
    }
    let mut bench = Bench {
        queue,
        runs,
        seconds,
        signal: TestSignal::new(RATE),
        results: Vec::new(),
        run: None,
        window: None,
    };
    let event_loop = EventLoop::new().expect("an event loop");
    event_loop
        .run_app(&mut bench)
        .expect("run the benchmark's event loop");
}
