//! A named shared-memory mapping that only the current user can open.

use std::io;

/// A read-write mapping of a named segment of at least `len` bytes, zero-filled when created.
pub(crate) struct Mapping {
    ptr: *mut u8,
    #[cfg(unix)]
    len: usize,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// SAFETY: the mapping is plain shared memory; every access through it goes through atomics.
unsafe impl Send for Mapping {}
// SAFETY: as above.
unsafe impl Sync for Mapping {}

impl Mapping {
    pub(crate) fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::ffi::CString;
    use std::time::{Duration, Instant};

    /// How long an opener waits for the creator to size a fresh segment.
    const SIZE_WAIT: Duration = Duration::from_secs(1);

    fn shm_name(name: &str) -> io::Result<CString> {
        CString::new(format!("/{name}")).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }

    impl Mapping {
        /// Creates the segment, or opens it if it already exists.
        pub(crate) fn create_or_open(name: &str, len: usize) -> io::Result<Mapping> {
            let c_name = shm_name(name)?;
            // SAFETY: `c_name` is a valid C string; the fd is closed on every path below.
            unsafe {
                let mut created = true;
                let mut fd = libc::shm_open(
                    c_name.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                    0o600 as libc::c_uint,
                );
                if fd < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
                    created = false;
                    fd = libc::shm_open(c_name.as_ptr(), libc::O_RDWR, 0);
                }
                if fd < 0 {
                    return Err(io::Error::last_os_error());
                }
                let result = Self::size_and_map(fd, len, created);
                libc::close(fd);
                result
            }
        }

        /// # Safety
        /// `fd` must be an open shared-memory descriptor.
        unsafe fn size_and_map(fd: libc::c_int, len: usize, created: bool) -> io::Result<Mapping> {
            // SAFETY: the caller passes an open descriptor; `stat` is written by `fstat`.
            unsafe {
                if created {
                    if libc::ftruncate(fd, len as libc::off_t) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                } else {
                    // The creator may not have sized it yet.
                    let start = Instant::now();
                    loop {
                        let mut stat: libc::stat = std::mem::zeroed();
                        if libc::fstat(fd, &mut stat) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        let size = stat.st_size as usize;
                        if size >= len {
                            break;
                        }
                        if size != 0 || start.elapsed() > SIZE_WAIT {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("shared-memory table is {size} bytes, expected {len}"),
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                let ptr = libc::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                );
                if ptr == libc::MAP_FAILED {
                    return Err(io::Error::last_os_error());
                }
                Ok(Mapping {
                    ptr: ptr.cast(),
                    len,
                })
            }
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: `ptr`/`len` came from a successful `mmap`.
            unsafe {
                libc::munmap(self.ptr.cast(), self.len);
            }
        }
    }

    /// Removes the name, so the next opener creates a fresh segment. Existing mappings stay valid.
    pub(crate) fn unlink(name: &str) {
        if let Ok(c_name) = shm_name(name) {
            // SAFETY: `c_name` is a valid C string.
            unsafe {
                libc::shm_unlink(c_name.as_ptr());
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_BASIC_INFORMATION,
        MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile, VirtualQuery,
    };

    impl Mapping {
        /// Creates the segment, or opens it if it already exists.
        ///
        /// The segment lives in the session's `Local\` namespace with the default security
        /// descriptor, which grants access to the current user (plus SYSTEM and administrators).
        pub(crate) fn create_or_open(name: &str, len: usize) -> io::Result<Mapping> {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            // SAFETY: `wide` is NUL-terminated; handles are closed on every error path.
            unsafe {
                let handle = CreateFileMappingW(
                    windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
                    std::ptr::null(),
                    PAGE_READWRITE,
                    (len as u64 >> 32) as u32,
                    len as u32,
                    wide.as_ptr(),
                );
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let existed = GetLastError() == ERROR_ALREADY_EXISTS;
                let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, 0);
                if view.Value.is_null() {
                    let error = io::Error::last_os_error();
                    CloseHandle(handle);
                    return Err(error);
                }
                let mapping = Mapping {
                    ptr: view.Value.cast(),
                    handle,
                };
                if existed {
                    let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
                    let got =
                        VirtualQuery(view.Value, &mut info, size_of::<MEMORY_BASIC_INFORMATION>());
                    if got == 0 || info.RegionSize < len {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "shared-memory table is {} bytes, expected {len}",
                                info.RegionSize
                            ),
                        ));
                    }
                }
                Ok(mapping)
            }
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: the view and handle came from successful calls above.
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.ptr.cast(),
                });
                CloseHandle(self.handle);
            }
        }
    }

    /// Named mappings vanish with their last handle on Windows, so there is nothing to remove.
    pub(crate) fn unlink(_name: &str) {}
}

pub(crate) use imp::unlink;
