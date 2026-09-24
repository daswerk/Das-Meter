//! The shared-memory table that carries audio from Send Plugins to the app.
//!
//! See ADR 0003 (`docs/adr/0003-send-plugin-shared-memory-table.md`).

/// Name of the shared-memory table on macOS. The layout version is part of the name.
pub const TABLE_NAME: &str = "dasmeter.v1";

/// Name of the shared-memory table on Windows, in the session namespace.
pub const TABLE_NAME_WINDOWS: &str = r"Local\dasmeter.v1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_names_fit_the_macos_sandbox_limit() {
        // A sandboxed macOS process can only open names of 31 characters or less.
        assert!(TABLE_NAME.len() <= 31);
        assert!(TABLE_NAME_WINDOWS.len() <= 31);
    }

    #[test]
    fn windows_name_is_the_macos_name_in_the_session_namespace() {
        assert_eq!(TABLE_NAME_WINDOWS, format!(r"Local\{TABLE_NAME}"));
    }
}
