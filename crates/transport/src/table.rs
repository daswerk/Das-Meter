//! Opening the table and reaching its header and slots.

use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::time::{Duration, Instant};

use crate::layout::*;
use crate::shm::Mapping;

/// How long an opener waits for another process to finish initialising the header.
const INIT_WAIT: Duration = Duration::from_secs(1);

/// An open, validated table. Cheap to clone; the mapping lives until the last clone drops.
#[derive(Clone)]
pub(crate) struct Table {
    mapping: Arc<Mapping>,
}

fn invalid(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

impl Table {
    /// Opens the table called `name`, creating it if this side comes first.
    pub(crate) fn open(name: &str) -> io::Result<Table> {
        let mapping = Mapping::create_or_open(&crate::platform_name(name), TABLE_SIZE)?;
        let table = Table {
            mapping: Arc::new(mapping),
        };
        table.initialise_or_validate()?;
        Ok(table)
    }

    pub(crate) fn header(&self) -> &Header {
        // SAFETY: the mapping is page-aligned and at least TABLE_SIZE bytes; Header is all atomics.
        unsafe { &*self.mapping.as_ptr().cast::<Header>() }
    }

    pub(crate) fn slot(&self, index: usize) -> &Slot {
        assert!(index < SLOT_COUNT);
        // SAFETY: in bounds per the assert and TABLE_SIZE; slots are 64-byte aligned and all atomics.
        unsafe {
            &*self
                .mapping
                .as_ptr()
                .add(HEADER_SIZE + index * SLOT_SIZE)
                .cast::<Slot>()
        }
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = (usize, &Slot)> {
        (0..SLOT_COUNT).map(|index| (index, self.slot(index)))
    }

    fn initialise_or_validate(&self) -> io::Result<()> {
        let header = self.header();
        let start = Instant::now();
        loop {
            match header.init.load(Acquire) {
                INIT_EMPTY => {
                    if header
                        .init
                        .compare_exchange(INIT_EMPTY, INIT_BUSY, AcqRel, Acquire)
                        .is_ok()
                    {
                        self.write_header();
                        return Ok(());
                    }
                }
                INIT_BUSY if start.elapsed() > INIT_WAIT => {
                    // The initialiser died half-way. The header holds only constants, so
                    // writing them again is safe even if it is merely slow.
                    self.write_header();
                    return Ok(());
                }
                INIT_BUSY => std::thread::sleep(Duration::from_millis(1)),
                INIT_READY => return self.validate(),
                other => return Err(invalid(format!("table header has init state {other}"))),
            }
        }
    }

    fn write_header(&self) {
        let header = self.header();
        header.layout_version.store(LAYOUT_VERSION, Relaxed);
        header.slot_count.store(SLOT_COUNT as u32, Relaxed);
        header.ring_frames.store(RING_FRAMES as u32, Relaxed);
        header.slot_size.store(SLOT_SIZE as u32, Relaxed);
        header.header_size.store(HEADER_SIZE as u32, Relaxed);
        header.magic.store(MAGIC, Relaxed);
        header.init.store(INIT_READY, Release);
    }

    fn validate(&self) -> io::Result<()> {
        let header = self.header();
        let expect = |what: &str, found: u64, wanted: u64| {
            if found == wanted {
                Ok(())
            } else {
                Err(invalid(format!(
                    "table {what} is {found}, expected {wanted}"
                )))
            }
        };
        expect("magic", header.magic.load(Relaxed), MAGIC)?;
        expect(
            "layout version",
            header.layout_version.load(Relaxed).into(),
            LAYOUT_VERSION.into(),
        )?;
        expect(
            "slot count",
            header.slot_count.load(Relaxed).into(),
            SLOT_COUNT as u64,
        )?;
        expect(
            "ring size",
            header.ring_frames.load(Relaxed).into(),
            RING_FRAMES as u64,
        )?;
        expect(
            "slot size",
            header.slot_size.load(Relaxed).into(),
            SLOT_SIZE as u64,
        )?;
        expect(
            "header size",
            header.header_size.load(Relaxed).into(),
            HEADER_SIZE as u64,
        )?;
        Ok(())
    }
}

/// What a Send Plugin tells the app about itself. It can change at any time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Details {
    /// Typed name, DAW track name or animal name. Cut to [`NAME_CAPACITY`] bytes.
    pub name: String,
    /// `0x00RRGGBB`.
    pub colour: u32,
    /// The track is mono; the ring still carries two identical channels.
    pub mono: bool,
    /// Frames per second of the audio in the ring.
    pub sample_rate: u32,
}

impl Slot {
    /// Writes the details under the seqlock. Only the slot's own writer calls this.
    pub(crate) fn write_details(&self, details: &Details) {
        let seq = self.details_seq.load(Relaxed);
        self.details_seq.store(seq.wrapping_add(1) | 1, Relaxed);
        std::sync::atomic::fence(Release);

        let name = truncate_utf8(&details.name, NAME_CAPACITY);
        let mut bytes = [0u8; NAME_CAPACITY];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        for (word, chunk) in self.name.iter().zip(bytes.chunks_exact(8)) {
            word.store(u64::from_le_bytes(chunk.try_into().unwrap()), Relaxed);
        }
        self.name_len.store(name.len() as u32, Relaxed);
        self.colour.store(details.colour & 0x00ff_ffff, Relaxed);
        self.flags
            .store(if details.mono { FLAG_MONO } else { 0 }, Relaxed);
        self.sample_rate.store(details.sample_rate, Relaxed);

        self.details_seq.store((seq | 1).wrapping_add(1), Release);
    }

    /// Reads a consistent copy of the ID and details, or `None` if they keep changing or are malformed.
    pub(crate) fn read_details(&self) -> Option<(u64, Details)> {
        for _ in 0..16 {
            let before = self.details_seq.load(Acquire);
            if before & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let len = self.name_len.load(Relaxed) as usize;
            let mut bytes = [0u8; NAME_CAPACITY];
            for (word, chunk) in self.name.iter().zip(bytes.chunks_exact_mut(8)) {
                chunk.copy_from_slice(&word.load(Relaxed).to_le_bytes());
            }
            let id = self.id.load(Relaxed);
            let colour = self.colour.load(Relaxed);
            let flags = self.flags.load(Relaxed);
            let sample_rate = self.sample_rate.load(Relaxed);
            std::sync::atomic::fence(Acquire);
            if self.details_seq.load(Relaxed) != before {
                continue;
            }
            if len > NAME_CAPACITY || colour > 0x00ff_ffff || flags & !FLAG_MONO != 0 {
                return None;
            }
            let name = std::str::from_utf8(&bytes[..len]).ok()?.to_owned();
            let details = Details {
                name,
                colour,
                mono: flags & FLAG_MONO != 0,
                sample_rate,
            };
            return Some((id, details));
        }
        None
    }
}

/// The longest prefix of `s` that fits in `max` bytes without splitting a character.
fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_names_are_cut_at_a_character_boundary() {
        // "ö" is two bytes, so byte 64 falls inside the 32nd "ö".
        let name = format!("a{}", "ö".repeat(40));
        let cut = truncate_utf8(&name, NAME_CAPACITY);
        assert_eq!(cut, format!("a{}", "ö".repeat(31)));
        assert_eq!(truncate_utf8("Otter", NAME_CAPACITY), "Otter");
    }
}
