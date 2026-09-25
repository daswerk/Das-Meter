//! Naming, colour, animal uniqueness, the duplicate-ID response and saved state.

use dasmeter_send::identity::{Identity, Other, Random, Track, is_animal};

/// Counts up from a seed, so picks are repeatable.
struct Seq(u64);

impl Random for Seq {
    fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0 - 1
    }
}

fn track(name: Option<&str>, colour: Option<u32>) -> Track {
    Track {
        name: name.map(str::to_owned),
        colour,
    }
}

fn other(id: u64, name: &str) -> Other {
    Other {
        id,
        name: name.to_owned(),
    }
}

#[test]
fn a_typed_name_wins_over_the_track_name() {
    let mut identity = Identity {
        typed_name: "Lead vox".into(),
        ..Identity::default()
    };
    let name = identity.name(&track(Some("Audio 3"), None), &[], &mut Seq(0));
    assert_eq!(name, "Lead vox");
}

#[test]
fn the_track_name_is_used_when_nothing_is_typed() {
    let mut identity = Identity {
        typed_name: "   ".into(),
        ..Identity::default()
    };
    let name = identity.name(&track(Some("Drums"), None), &[], &mut Seq(0));
    assert_eq!(name, "Drums");
    assert_eq!(
        identity.animal, None,
        "no animal is picked while a name exists"
    );
}

#[test]
fn an_animal_is_used_without_a_typed_or_track_name_and_stays() {
    let mut identity = Identity::default();
    let first = identity.name(&track(None, None), &[], &mut Seq(0));
    assert!(is_animal(&first), "{first} is an animal");
    let again = identity.name(&track(Some(""), None), &[], &mut Seq(50));
    assert_eq!(again, first, "the animal is kept, not picked again");
}

#[test]
fn animals_are_unique_among_live_send_plugins() {
    // Seq(0) would pick the first animal; make it and the next one taken.
    let mut probe = Identity::default();
    let first = probe.name(&track(None, None), &[], &mut Seq(0));
    let mut probe = Identity::default();
    let second = probe.name(&track(None, None), &[], &mut Seq(1));
    let others = [other(1, &first), other(2, &second)];

    let mut identity = Identity::default();
    let name = identity.name(&track(None, None), &others, &mut Seq(0));
    assert!(is_animal(&name));
    assert_ne!(name, first);
    assert_ne!(name, second);
}

#[test]
fn clashing_track_names_get_a_number() {
    let mut identity = Identity::default();
    let others = [other(1, "Bass"), other(2, "Bass 2")];
    let name = identity.name(&track(Some("Bass"), None), &others, &mut Seq(0));
    assert_eq!(name, "Bass 3");
}

#[test]
fn the_track_colour_wins_over_the_random_one() {
    let mut identity = Identity {
        random_colour: Some(0x123456),
        ..Identity::default()
    };
    assert_eq!(
        identity.colour(&track(None, Some(0xff00aa)), &mut Seq(0)),
        0xff00aa
    );
    assert_eq!(identity.colour(&track(None, None), &mut Seq(0)), 0x123456);
}

#[test]
fn a_random_colour_is_picked_once_and_kept() {
    let mut identity = Identity::default();
    let first = identity.colour(&track(None, None), &mut Seq(7));
    assert_eq!(identity.random_colour, Some(first));
    assert_eq!(identity.colour(&track(None, None), &mut Seq(200)), first);
    assert!(first <= 0xff_ffff);
}

#[test]
fn a_duplicate_id_gets_the_fresh_id_and_a_fresh_animal() {
    // A duplicated track: the copy loads the same state as the original.
    let mut original = Identity {
        id: Some(42),
        ..Identity::default()
    };
    let animal = original.name(&track(None, None), &[], &mut Seq(3));
    let mut copy = original.clone();

    copy.id_was_taken(99, &track(None, None), &[other(42, &animal)], &mut Seq(3));
    assert_eq!(copy.id, Some(99));
    let name = copy.name(&track(None, None), &[other(42, &animal)], &mut Seq(0));
    assert!(is_animal(&name));
    assert_ne!(name, animal);
}

#[test]
fn a_duplicate_id_keeps_a_typed_or_track_name() {
    let mut copy = Identity {
        id: Some(42),
        typed_name: "Kick".into(),
        animal: Some("Otter".into()),
        ..Identity::default()
    };
    copy.id_was_taken(99, &track(None, None), &[], &mut Seq(0));
    assert_eq!(copy.id, Some(99));
    assert_eq!(
        copy.animal.as_deref(),
        Some("Otter"),
        "the animal isn't shown, so it stays"
    );

    let mut copy = Identity {
        id: Some(42),
        animal: Some("Otter".into()),
        ..Identity::default()
    };
    copy.id_was_taken(99, &track(Some("Snare"), None), &[], &mut Seq(0));
    assert_eq!(copy.animal.as_deref(), Some("Otter"));
}

#[test]
fn state_round_trips() {
    let identities = [
        Identity::default(),
        Identity {
            id: Some(0xdead_beef_cafe_f00d),
            typed_name: "Grüße 🎛".into(),
            animal: Some("Otter".into()),
            random_colour: Some(0x33aa77),
        },
        Identity {
            id: None,
            typed_name: String::new(),
            animal: Some("Wombat".into()),
            random_colour: None,
        },
    ];
    for identity in identities {
        let bytes = identity.save();
        assert_eq!(Identity::load(&bytes), Ok(identity));
    }
}

#[test]
fn bad_state_is_refused() {
    let good = Identity {
        id: Some(1),
        typed_name: "Keys".into(),
        ..Identity::default()
    }
    .save();
    assert!(Identity::load(b"").is_err());
    assert!(Identity::load(b"NOPE\x01\x00\x00\x00\x00\x00\x00").is_err());
    for len in 0..good.len() {
        assert!(Identity::load(&good[..len]).is_err(), "truncated to {len}");
    }
    let mut newer = good.clone();
    newer[4] = 2;
    assert!(
        Identity::load(&newer).is_err(),
        "a newer version is refused"
    );
}

#[test]
fn the_oldest_send_plugin_keeps_the_plain_track_name() {
    let mut older = Identity {
        id: Some(1),
        ..Identity::default()
    };
    let mut newer = Identity {
        id: Some(2),
        ..Identity::default()
    };
    let bass = track(Some("Bass"), None);
    // Even if both showed "Bass" for a moment, they settle without swapping.
    let older_name = older.name(&bass, &[other(2, "Bass")], &mut Seq(0));
    let newer_name = newer.name(&bass, &[other(1, "Bass")], &mut Seq(0));
    assert_eq!(
        (older_name.as_str(), newer_name.as_str()),
        ("Bass", "Bass 2")
    );
    assert_eq!(
        older.name(&bass, &[other(2, "Bass 2")], &mut Seq(0)),
        "Bass"
    );
    assert_eq!(
        newer.name(&bass, &[other(1, "Bass")], &mut Seq(0)),
        "Bass 2"
    );
}
