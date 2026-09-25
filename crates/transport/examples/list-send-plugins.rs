//! Lists the Send Plugins in the table, as the app would see them, once a
//! second for a few seconds. For checking a Send Plugin in a DAW without the app.
//!
//!   cargo run -p dasmeter-transport --example list-send-plugins [SECONDS]

use std::time::{Duration, Instant};

use dasmeter_transport::Reader;

fn main() -> std::io::Result<()> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let mut reader = Reader::open()?;
    for _ in 0..seconds {
        reader.heartbeat();
        let slots = reader.slots(Instant::now());
        println!("{} Send Plugin(s)", slots.len());
        for slot in slots {
            println!(
                "  {:016x}  {:<24} #{:06x}  {}  {} Hz  {:?}  pid {}",
                slot.id,
                slot.details.name,
                slot.details.colour,
                if slot.details.mono {
                    "mono  "
                } else {
                    "stereo"
                },
                slot.details.sample_rate,
                slot.state,
                slot.host_pid,
            );
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}
