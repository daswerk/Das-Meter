//! Whether a Send Plugin's host process is still running.

/// Watches host processes by PID.
///
/// On Windows it keeps a handle to each process it has seen, so a PID reused by a
/// new process is never mistaken for the old host. On macOS it polls with
/// `kill(pid, 0)`; a reused PID within the heartbeat timeout is not caught here,
/// but the 2 s heartbeat timeout still marks the slot gone.
#[derive(Default)]
pub(crate) struct ProcessWatch {
    #[cfg(windows)]
    handles: std::collections::HashMap<u32, win::Handle>,
}

impl ProcessWatch {
    /// Whether `pid` is still running. Forgets processes it's no longer asked about via [`Self::retain`].
    pub(crate) fn is_alive(&mut self, pid: u32) -> bool {
        #[cfg(windows)]
        {
            use std::collections::hash_map::Entry;
            match self.handles.entry(pid) {
                Entry::Occupied(entry) => entry.get().is_running(),
                Entry::Vacant(entry) => match win::Handle::open(pid) {
                    Some(handle) => entry.insert(handle).is_running(),
                    None => false,
                },
            }
        }
        #[cfg(not(windows))]
        {
            process_alive(pid)
        }
    }

    /// Drops watches for PIDs not in `pids`.
    pub(crate) fn retain(&mut self, pids: &[u32]) {
        #[cfg(windows)]
        self.handles.retain(|pid, _| pids.contains(pid));
        #[cfg(not(windows))]
        let _ = pids;
    }
}

/// A one-off check whether `pid` is running.
pub(crate) fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks that the process exists and may be signalled.
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        win::Handle::open(pid).is_some_and(|handle| handle.is_running())
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    pub(crate) struct Handle(HANDLE);

    impl Handle {
        pub(crate) fn open(pid: u32) -> Option<Handle> {
            // SAFETY: plain Win32 call; a null handle means failure.
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            (!handle.is_null()).then_some(Handle(handle))
        }

        pub(crate) fn is_running(&self) -> bool {
            // SAFETY: the handle is open for SYNCHRONIZE; a zero timeout never blocks.
            unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from `OpenProcess`.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    // SAFETY: a process handle can be used from any thread.
    unsafe impl Send for Handle {}
}
