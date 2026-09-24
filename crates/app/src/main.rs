//! The Das-Meter app: turns the app core's scene into windows and Meters.

fn version_line() -> String {
    format!("Das-Meter {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    println!("{}", version_line());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_names_the_app() {
        assert!(version_line().starts_with("Das-Meter "));
    }
}
