//! Who a Send Plugin is: its ID, name and colour, and how they survive project reloads.
//!
//! This is plain logic with no CLAP or shared memory in it, so it can be tested
//! on its own. The plugin feeds it what the DAW and the transport report.

use crate::animals::ANIMALS;

/// What a Send Plugin keeps in its saved state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    /// The transport ID, once a slot has been claimed.
    pub id: Option<u64>,
    /// The name the user typed. Empty means none.
    pub typed_name: String,
    /// The animal name, picked the first time one is needed.
    pub animal: Option<String>,
    /// The random colour (`0x00RRGGBB`), picked the first time one is needed.
    pub random_colour: Option<u32>,
}

/// What the DAW reports about the track the plugin sits on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub name: Option<String>,
    /// `0x00RRGGBB`.
    pub colour: Option<u32>,
}

/// Another live Send Plugin, as the transport lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Other {
    pub id: u64,
    pub name: String,
}

/// A source of randomness, so tests can pick animals and colours deterministically.
pub trait Random {
    fn next(&mut self) -> u64;
}

/// Randomness from the standard library's hasher seeds. Good enough for picking names.
pub struct StdRandom;

impl Random for StdRandom {
    fn next(&mut self) -> u64 {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        hasher.finish()
    }
}

impl Identity {
    /// The name to show: typed, else the track's, else the animal (picked now if
    /// there is none yet).
    ///
    /// A track name another live Send Plugin with a lower ID already shows gets a
    /// number, so the oldest keeps the plain name and the rest settle on "Bass 2",
    /// "Bass 3"… without swapping back and forth. `others` excludes this plugin.
    pub fn name(&mut self, track: &Track, others: &[Other], random: &mut impl Random) -> String {
        let typed = self.typed_name.trim();
        if !typed.is_empty() {
            return typed.to_owned();
        }
        if let Some(track_name) = track
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            let own = self.id.unwrap_or(u64::MAX);
            let older: Vec<Other> = others.iter().filter(|o| o.id < own).cloned().collect();
            return numbered(track_name, &older);
        }
        self.animal(others, random).to_owned()
    }

    /// The colour to show: the track's, else the random one (picked now if there is none yet).
    pub fn colour(&mut self, track: &Track, random: &mut impl Random) -> u32 {
        if let Some(colour) = track.colour {
            return colour & 0x00ff_ffff;
        }
        *self
            .random_colour
            .get_or_insert_with(|| random_colour(random))
    }

    /// Whether the name shown is the animal (no typed name, no track name).
    pub fn shows_animal(&self, track: &Track) -> bool {
        self.typed_name.trim().is_empty()
            && track.name.as_deref().is_none_or(|n| n.trim().is_empty())
    }

    /// The transport gave a fresh ID because ours was live in another Send Plugin
    /// (a duplicated track). Store the new ID, and pick a fresh animal if the name
    /// shown is an animal, so the two copies can be told apart.
    pub fn id_was_taken(
        &mut self,
        new_id: u64,
        track: &Track,
        others: &[Other],
        random: &mut impl Random,
    ) {
        self.id = Some(new_id);
        if self.shows_animal(track) {
            self.animal = None;
            self.animal(others, random);
        }
    }

    fn animal(&mut self, others: &[Other], random: &mut impl Random) -> &str {
        self.animal
            .get_or_insert_with(|| pick_animal(others, random))
            .as_str()
    }
}

/// `name`, or `name 2`, `name 3`… if other live Send Plugins already show it.
fn numbered(name: &str, others: &[Other]) -> String {
    let taken = |candidate: &str| others.iter().any(|o| o.name == candidate);
    if !taken(name) {
        return name.to_owned();
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|candidate| !taken(candidate))
        .expect("some number is free")
}

/// An animal no live Send Plugin shows, starting from a random one. If all are
/// taken (more than ~100 live Send Plugins), a numbered animal.
fn pick_animal(others: &[Other], random: &mut impl Random) -> String {
    let start = (random.next() % ANIMALS.len() as u64) as usize;
    let taken = |name: &str| others.iter().any(|o| o.name == name);
    (0..ANIMALS.len())
        .map(|i| ANIMALS[(start + i) % ANIMALS.len()])
        .find(|animal| !taken(animal))
        .map_or_else(|| numbered(ANIMALS[start], others), str::to_owned)
}

/// Whether `name` is one of the animal names.
pub fn is_animal(name: &str) -> bool {
    ANIMALS.contains(&name)
}

/// A random colour bright enough to read on a dark background: a random hue at
/// fixed saturation and lightness.
fn random_colour(random: &mut impl Random) -> u32 {
    let hue = (random.next() % 360) as f32;
    let (s, l) = (0.65_f32, 0.6_f32);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// Magic at the start of saved state: `"DMSP"`.
const STATE_MAGIC: [u8; 4] = *b"DMSP";
/// Version of the saved-state format.
const STATE_VERSION: u8 = 1;

/// Why saved state couldn't be read.
#[derive(Debug, PartialEq, Eq)]
pub struct BadState;

impl Identity {
    /// Serialises the identity for the plugin's saved state.
    ///
    /// Format: `"DMSP"`, version byte, then optional fields as a presence byte
    /// plus value (ID `u64` LE, colour `u32` LE), then typed name and animal as
    /// `u16` LE length plus UTF-8.
    pub fn save(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&STATE_MAGIC);
        out.push(STATE_VERSION);
        match self.id {
            Some(id) => {
                out.push(1);
                out.extend_from_slice(&id.to_le_bytes());
            }
            None => out.push(0),
        }
        match self.random_colour {
            Some(colour) => {
                out.push(1);
                out.extend_from_slice(&colour.to_le_bytes());
            }
            None => out.push(0),
        }
        write_str(&mut out, &self.typed_name);
        write_str(&mut out, self.animal.as_deref().unwrap_or(""));
        out
    }

    /// Reads state written by [`Identity::save`]. Newer versions are refused.
    pub fn load(bytes: &[u8]) -> Result<Identity, BadState> {
        let mut r = Bytes(bytes);
        if r.take(4)? != STATE_MAGIC || r.u8()? != STATE_VERSION {
            return Err(BadState);
        }
        let id = match r.u8()? {
            0 => None,
            1 => Some(u64::from_le_bytes(r.array()?)).filter(|&id| id != 0),
            _ => return Err(BadState),
        };
        let random_colour = match r.u8()? {
            0 => None,
            1 => Some(u32::from_le_bytes(r.array()?) & 0x00ff_ffff),
            _ => return Err(BadState),
        };
        let typed_name = r.str()?;
        let animal = Some(r.str()?).filter(|a| !a.is_empty());
        Ok(Identity {
            id,
            typed_name,
            animal,
            random_colour,
        })
    }
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    let mut end = s.len().min(u16::MAX as usize);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    out.extend_from_slice(&(end as u16).to_le_bytes());
    out.extend_from_slice(&s.as_bytes()[..end]);
}

struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BadState> {
        if self.0.len() < n {
            return Err(BadState);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, BadState> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], BadState> {
        Ok(self.take(N)?.try_into().expect("took N bytes"))
    }

    fn str(&mut self) -> Result<String, BadState> {
        let len = u16::from_le_bytes(self.array()?) as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| BadState)
    }
}
