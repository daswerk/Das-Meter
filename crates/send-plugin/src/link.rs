//! The main thread's part: the Send Plugin's slot in the transport, its
//! identity, the heartbeat and the connection status.
//!
//! Nothing here touches CLAP, so the plugin's behaviour against the transport
//! can be tested with two `Link`s in one process.

use std::thread::JoinHandle;
use std::time::Instant;

use dasmeter_transport::{
    AudioWriter, Details, HEARTBEAT_INTERVAL, Reader, SlotState, TABLE_NAME, Writer,
};

use crate::identity::{Identity, Other, Random, StdRandom, Track};

/// What the plugin window says about the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The app is running and can see this Send Plugin.
    Connected,
    /// No app heartbeat: "Das-Meter isn't running".
    AppNotRunning,
    /// The table is from a newer layout than this Send Plugin writes, or it
    /// couldn't be opened: "Update Das-Meter".
    UpdateApp,
}

impl Status {
    pub fn text(self) -> &'static str {
        match self {
            Status::Connected => "Connected",
            Status::AppNotRunning => "Das-Meter isn't running",
            Status::UpdateApp => "Update Das-Meter",
        }
    }
}

/// A Send Plugin's link to the app.
pub struct Link<R: Random = StdRandom> {
    table: String,
    identity: Identity,
    track: Track,
    mono: bool,
    sample_rate: u32,
    writer: Option<Writer>,
    /// For listing the other Send Plugins' names. Opened on first use.
    reader: Option<Reader>,
    details: Option<Details>,
    status: Status,
    random: R,
    /// Whether to bump the heartbeat from a thread of our own: the host has no timer.
    own_heartbeat: bool,
    heartbeat_threads: Vec<JoinHandle<()>>,
}

impl Link<StdRandom> {
    pub fn new() -> Link<StdRandom> {
        Link::with(TABLE_NAME, StdRandom)
    }
}

impl Default for Link<StdRandom> {
    fn default() -> Self {
        Link::new()
    }
}

impl<R: Random> Link<R> {
    /// A link on a table with another name and a given randomness, for tests.
    pub fn with(table: &str, random: R) -> Link<R> {
        Link {
            table: table.to_owned(),
            identity: Identity::default(),
            track: Track::default(),
            mono: false,
            sample_rate: 48_000,
            writer: None,
            reader: None,
            details: None,
            status: Status::AppNotRunning,
            random,
            own_heartbeat: false,
            heartbeat_threads: Vec::new(),
        }
    }

    /// The host gives the plugin no main-thread timer (clap-wrapper's AU, for one),
    /// so [`Link::tick`] won't be called: bump the heartbeat from a thread instead,
    /// or the app would take the Send Plugin for gone.
    pub fn use_own_heartbeat(&mut self) {
        self.own_heartbeat = true;
        self.start_heartbeat_thread();
    }

    fn start_heartbeat_thread(&mut self) {
        let Some(writer) = self.writer.as_ref().filter(|_| self.own_heartbeat) else {
            return;
        };
        let heartbeat = writer.heartbeat_handle();
        self.heartbeat_threads
            .retain(|thread| !thread.is_finished());
        let thread = std::thread::Builder::new()
            .name("Das-Meter Send heartbeat".into())
            .spawn(move || {
                // Ends once the slot is released (the Link dropped or reclaimed).
                while heartbeat.beat() {
                    std::thread::park_timeout(HEARTBEAT_INTERVAL);
                }
            });
        if let Ok(thread) = thread {
            self.heartbeat_threads.push(thread);
        }
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// The name the app shows for this Send Plugin, once it has a slot.
    pub fn shown_name(&self) -> Option<&str> {
        self.details.as_ref().map(|details| details.name.as_str())
    }

    /// The details the app sees now, once connected.
    pub fn details(&self) -> Option<&Details> {
        self.details.as_ref()
    }

    /// Claims a slot (if there isn't one yet) and returns a handle for the audio
    /// thread. Call it on activation, with the host's sample rate.
    ///
    /// Returns `None` if no slot could be claimed; the plugin then only passes
    /// audio through, and tries again on the next activation.
    pub fn activate(&mut self, sample_rate: u32) -> Option<AudioWriter> {
        self.sample_rate = sample_rate;
        if self.writer.is_none() {
            self.claim();
        }
        self.refresh(Instant::now());
        self.writer.as_ref().map(Writer::audio)
    }

    fn claim(&mut self) {
        let others = self.others(Instant::now());
        let details = self.build_details(&others);
        match Writer::claim_named(&self.table, self.identity.id, &details) {
            Ok(claimed) => {
                if claimed.id_was_taken {
                    self.identity
                        .id_was_taken(claimed.id, &self.track, &others, &mut self.random);
                } else {
                    self.identity.id = Some(claimed.id);
                }
                self.writer = Some(claimed.writer);
                self.start_heartbeat_thread();
            }
            Err(error) => {
                self.status = if error.kind() == std::io::ErrorKind::InvalidData {
                    Status::UpdateApp
                } else {
                    Status::AppNotRunning
                };
            }
        }
    }

    /// Call about every 250 ms from a main-thread timer: bumps the heartbeat,
    /// checks for the app, and republishes the name if another Send Plugin now
    /// clashes with it.
    pub fn tick(&mut self, now: Instant) {
        if let Some(writer) = &self.writer {
            writer.heartbeat();
        }
        self.refresh(now);
    }

    /// The DAW reported the track's name or colour.
    pub fn set_track(&mut self, track: Track) {
        if track != self.track {
            self.track = track;
            self.refresh(Instant::now());
        }
    }

    /// The track is mono (or stereo again).
    pub fn set_mono(&mut self, mono: bool) {
        if mono != self.mono {
            self.mono = mono;
            self.refresh(Instant::now());
        }
    }

    /// The user typed a name. Empty clears it.
    pub fn set_typed_name(&mut self, name: &str) {
        if name != self.identity.typed_name {
            self.identity.typed_name = name.to_owned();
            self.refresh(Instant::now());
        }
    }

    /// The plugin's saved state.
    pub fn save(&self) -> Vec<u8> {
        self.identity.save()
    }

    /// Restores saved state. If a slot was already claimed under another ID,
    /// it is released and claimed again under the saved one; the returned
    /// handle replaces the audio thread's (`None` if nothing changed or no slot).
    pub fn load(&mut self, bytes: &[u8]) -> Result<Option<AudioWriter>, crate::identity::BadState> {
        let identity = Identity::load(bytes)?;
        let reclaim = self.writer.is_some() && identity.id != self.identity.id;
        self.identity = identity;
        if reclaim {
            self.writer = None;
            self.claim();
            self.refresh(Instant::now());
            return Ok(self.writer.as_ref().map(Writer::audio));
        }
        self.refresh(Instant::now());
        Ok(None)
    }

    /// The other live Send Plugins in the table, without this one and gone ones.
    fn others(&mut self, now: Instant) -> Vec<Other> {
        if self.reader.is_none() {
            self.reader = Reader::open_named(&self.table).ok();
        }
        let own = self.writer.as_ref().map(Writer::id);
        let Some(reader) = &mut self.reader else {
            return Vec::new();
        };
        reader
            .slots(now)
            .into_iter()
            // A crashed host's Send Plugins stay listed as gone; their names are free.
            .filter(|slot| Some(slot.id) != own && slot.state != SlotState::Gone)
            .map(|slot| Other {
                id: slot.id,
                name: slot.details.name,
            })
            .collect()
    }

    fn build_details(&mut self, others: &[Other]) -> Details {
        Details {
            name: self.identity.name(&self.track, others, &mut self.random),
            colour: self.identity.colour(&self.track, &mut self.random),
            mono: self.mono,
            sample_rate: self.sample_rate,
        }
    }

    fn refresh(&mut self, now: Instant) {
        if self.writer.is_none() {
            return;
        }
        let others = self.others(now);
        let details = self.build_details(&others);
        let Some(writer) = &mut self.writer else {
            return;
        };
        if self.details.as_ref() != Some(&details) {
            writer.set_details(&details);
            self.details = Some(details);
        }
        self.status = if writer.app_running(now) {
            Status::Connected
        } else {
            Status::AppNotRunning
        };
    }
}

impl<R: Random> Drop for Link<R> {
    fn drop(&mut self) {
        // Release the slot, then wait for the heartbeat threads to see it: the
        // plugin's code may be unloaded right after.
        self.writer = None;
        for thread in self.heartbeat_threads.drain(..) {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}
