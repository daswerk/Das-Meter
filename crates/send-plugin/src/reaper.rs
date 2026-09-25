//! REAPER's own API, for the track colour.
//!
//! REAPER's CLAP track-info reports the track colour as the theme draws it,
//! tinted towards grey (a pure red track arrives as `#b12121`), so the Send
//! Plugin would show a colour that matches nothing the user picked. REAPER also
//! hands CLAP plugins its extension API ("cockos.reaper_extension", see the
//! REAPER SDK's `reaper_plugin.h`), through which the plugin reads the colour
//! the user actually set.

use std::ffi::{CStr, c_char, c_int, c_void};

use clack_plugin::extensions::{Extension, HostExtensionSide, RawExtension};
use clack_plugin::prelude::*;

/// The start of REAPER's `reaper_plugin_info_t`, as far as we use it.
#[derive(Clone, Copy)]
#[repr(C)]
struct PluginInfo {
    caller_version: c_int,
    hwnd_main: *mut c_void,
    register: Option<unsafe extern "C" fn(name: *const c_char, info: *mut c_void) -> c_int>,
    get_func: Option<unsafe extern "C" fn(name: *const c_char) -> *mut c_void>,
}

/// REAPER's API, when the host is REAPER.
#[derive(Copy, Clone)]
pub struct HostReaper(RawExtension<HostExtensionSide, PluginInfo>);

// SAFETY: PluginInfo is repr(C) and matches the start of REAPER's struct.
unsafe impl Extension for HostReaper {
    const IDENTIFIERS: &[&CStr] = &[c"cockos.reaper_extension"];
    type ExtensionSide = HostExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        // SAFETY: the caller guarantees the pointer is REAPER's extension struct.
        Self(unsafe { raw.cast() })
    }
}

type GetContext = unsafe extern "C" fn(host: *const c_void, sel: c_int) -> *mut c_void;
type GetTrackColor = unsafe extern "C" fn(track: *mut c_void) -> c_int;
type ColorFromNative =
    unsafe extern "C" fn(col: c_int, r: *mut c_int, g: *mut c_int, b: *mut c_int);

/// `clap_get_reaper_context`'s selector for the track the plugin sits on.
const CONTEXT_TRACK: c_int = 1;
/// Set in a REAPER track colour when the user picked one.
const CUSTOM_COLOUR: c_int = 0x0100_0000;

impl HostReaper {
    /// The colour the user set on the plugin's track (`0x00RRGGBB`): `Some(None)`
    /// if the track has none, `None` if REAPER couldn't tell (not a track FX,
    /// or a REAPER older than 6.80).
    pub fn track_colour(&self, _host: &HostMainThreadHandle) -> Option<Option<u32>> {
        let get = |name: &CStr| -> Option<*mut c_void> {
            // SAFETY: the extension struct lives as long as the host, and
            // GetFunc may be called from the main thread, which the handle proves.
            let info = unsafe { self.0.as_ptr().as_ref() };
            let function = unsafe { info.get_func?(name.as_ptr()) };
            (!function.is_null()).then_some(function)
        };
        // SAFETY: the names and signatures are REAPER's documented API
        // (reaper_plugin.h and reaper_plugin_functions.h).
        unsafe {
            let context: GetContext = std::mem::transmute(get(c"clap_get_reaper_context")?);
            let track_colour: GetTrackColor = std::mem::transmute(get(c"GetTrackColor")?);
            let from_native: ColorFromNative = std::mem::transmute(get(c"ColorFromNative")?);

            let track = context(self.0.host_ptr().as_ptr().cast(), CONTEXT_TRACK);
            if track.is_null() {
                return None;
            }
            let native = track_colour(track);
            if native & CUSTOM_COLOUR == 0 {
                return Some(None);
            }
            let (mut r, mut g, mut b) = (0, 0, 0);
            from_native(native & !CUSTOM_COLOUR, &mut r, &mut g, &mut b);
            let channel = |c: c_int| c.clamp(0, 255) as u32;
            Some(Some((channel(r) << 16) | (channel(g) << 8) | channel(b)))
        }
    }
}
