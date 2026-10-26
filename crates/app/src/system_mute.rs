//! A/B on System Capture: while B plays, everything else on the computer
//! goes quiet, so the song is heard alone. Das-Meter itself (the song) plays on.
//!
//! On macOS a muted Core Audio process tap on every process but this one
//! (macOS 14.2+, the same System Audio Recording permission as System
//! Capture), read (and thrown away) through a private device of its own:
//! a tap mutes only while something reads it. The other apps' sound still
//! reaches System Capture; it just doesn't reach the speakers. On Windows every other audio session on the
//! default output is muted, and unmuted again afterwards (sessions the user
//! had muted stay muted); an app that starts playing while B plays isn't
//! muted. Elsewhere there's nothing to do it with yet.
//!
//! Dropping a [`SystemMute`] lets everything play again. If the app quits or
//! crashes on macOS, the system removes the tap with it.

/// The rest of the computer, muted until dropped.
pub struct SystemMute {
    #[cfg(target_os = "macos")]
    tap: imp::Tap,
    #[cfg(windows)]
    muted: Vec<windows::Win32::Media::Audio::ISimpleAudioVolume>,
}

impl SystemMute {
    /// Mutes every other app.
    pub fn start() -> Result<SystemMute, String> {
        imp::start()
    }
}

impl Drop for SystemMute {
    fn drop(&mut self) {
        imp::stop(self);
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, c_void};
    use std::mem::MaybeUninit;
    use std::ptr::NonNull;

    use objc2::AnyThread;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_core_audio::{
        AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
        AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
        AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
        AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
        AudioObjectPropertyAddress, CATapDescription, CATapMuteBehavior,
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
        kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
        kAudioAggregateDeviceUIDKey, kAudioHardwarePropertyTranslatePIDToProcessObject,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
        kAudioSubTapUIDKey,
    };
    use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};
    use objc2_core_foundation::CFDictionary;
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

    use super::SystemMute;

    /// The tap, the private device it's read through, and the reader.
    pub struct Tap {
        tap: AudioObjectID,
        device: AudioObjectID,
        reader: AudioDeviceIOProcID,
    }

    /// Reads the tap and throws it away: a muted tap only keeps the other
    /// apps from the speakers while something reads it.
    unsafe extern "C-unwind" fn ignore(
        _: AudioObjectID,
        _: NonNull<AudioTimeStamp>,
        _: NonNull<AudioBufferList>,
        _: NonNull<AudioTimeStamp>,
        _: NonNull<AudioBufferList>,
        _: NonNull<AudioTimeStamp>,
        _: *mut c_void,
    ) -> i32 {
        0
    }

    pub fn start() -> Result<SystemMute, String> {
        // This process as Core Audio knows it: it exists once it plays, and
        // the song is already running (silently) whenever B can be picked.
        let me = own_process()?;
        let exclude = NSArray::from_retained_slice(&[NSNumber::new_u32(me)]);
        let description = unsafe {
            CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &exclude,
            )
        };
        unsafe {
            description.setMuteBehavior(CATapMuteBehavior::Muted);
            description.setName(&NSString::from_str("Das-Meter A/B"));
            description.setPrivate(true);
        }
        let mut tap = MaybeUninit::<AudioObjectID>::uninit();
        let status = unsafe { AudioHardwareCreateProcessTap(Some(&description), tap.as_mut_ptr()) };
        check(status, "couldn't make the tap that mutes the other apps")?;
        let tap = unsafe { tap.assume_init() };
        let mut mute = Tap {
            tap,
            device: 0,
            reader: None,
        };

        // A private device that reads only the tap.
        let key = |k: &CStr| NSString::from_str(k.to_str().unwrap_or_default());
        let uid = unsafe { description.UUID().UUIDString() };
        let sub_tap = NSDictionary::<NSString, AnyObject>::from_slices(
            &[&*key(kAudioSubTapUIDKey)],
            &[as_object(&*uid)],
        );
        let taps = NSArray::from_retained_slice(&[sub_tap]);
        let yes = NSNumber::new_bool(true);
        let name = NSString::from_str("Das-Meter A/B");
        let device_uid = NSString::from_str(&format!(
            "berlin.daswerk.DasMeter.ab.{}",
            std::process::id()
        ));
        let properties = NSDictionary::<NSString, AnyObject>::from_slices(
            &[
                &*key(kAudioAggregateDeviceNameKey),
                &*key(kAudioAggregateDeviceUIDKey),
                &*key(kAudioAggregateDeviceTapListKey),
                &*key(kAudioAggregateDeviceTapAutoStartKey),
                &*key(kAudioAggregateDeviceIsPrivateKey),
            ],
            &[
                as_object(&*name),
                as_object(&*device_uid),
                as_object(&*taps),
                as_object(&*yes),
                as_object(&*yes),
            ],
        );
        // NSDictionary and CFDictionary are the same object.
        let properties: &CFDictionary =
            unsafe { &*(Retained::as_ptr(&properties).cast::<CFDictionary>()) };
        let status = unsafe {
            AudioHardwareCreateAggregateDevice(properties, NonNull::from(&mut mute.device))
        };
        check(status, "couldn't make the device that mutes the other apps")?;
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                mute.device,
                Some(ignore),
                std::ptr::null_mut(),
                NonNull::from(&mut mute.reader),
            )
        };
        check(status, "couldn't read the tap that mutes the other apps")?;
        let status = unsafe { AudioDeviceStart(mute.device, mute.reader) };
        check(status, "couldn't start muting the other apps")?;
        Ok(SystemMute { tap: mute })
    }

    pub fn stop(mute: &mut SystemMute) {
        let t = &mut mute.tap;
        unsafe {
            if t.reader.is_some() {
                AudioDeviceStop(t.device, t.reader);
                AudioDeviceDestroyIOProcID(t.device, t.reader);
                t.reader = None;
            }
            if t.device != 0 {
                AudioHardwareDestroyAggregateDevice(t.device);
                t.device = 0;
            }
            if t.tap != 0 {
                AudioHardwareDestroyProcessTap(t.tap);
                t.tap = 0;
            }
        }
    }

    impl Drop for Tap {
        /// A half-made mute (a later step failed) is taken apart again.
        fn drop(&mut self) {
            let mut mute = SystemMute {
                tap: Tap {
                    tap: std::mem::take(&mut self.tap),
                    device: std::mem::take(&mut self.device),
                    reader: self.reader.take(),
                },
            };
            stop(&mut mute);
            std::mem::forget(mute);
        }
    }

    fn as_object<T: AsRef<AnyObject> + ?Sized>(object: &T) -> &AnyObject {
        object.as_ref()
    }

    fn check(status: i32, what: &str) -> Result<(), String> {
        if status == 0 {
            Ok(())
        } else {
            Err(format!("{what} (Core Audio {status})"))
        }
    }

    fn own_process() -> Result<AudioObjectID, String> {
        let pid = std::process::id() as i32;
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut process: AudioObjectID = 0;
        let mut size = size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&address),
                size_of::<i32>() as u32,
                (&raw const pid).cast(),
                NonNull::from(&mut size),
                NonNull::from(&mut process).cast(),
            )
        };
        if status != 0 || process == 0 {
            return Err(format!(
                "Das-Meter isn't playing yet, so it can't keep itself heard (Core Audio {status})"
            ));
        }
        Ok(process)
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Media::Audio::{
        IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator, eConsole, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::core::Interface;

    use super::SystemMute;

    pub fn start() -> Result<SystemMute, String> {
        let me = std::process::id();
        let mut muted = Vec::new();
        unsafe {
            // Already initialised on the UI thread; the answer doesn't matter.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let fail = |e: windows::core::Error| format!("couldn't mute the other apps ({e})");
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(fail)?;
            let output = devices
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(fail)?;
            let manager: IAudioSessionManager2 = output.Activate(CLSCTX_ALL, None).map_err(fail)?;
            let sessions = manager.GetSessionEnumerator().map_err(fail)?;
            for i in 0..sessions.GetCount().map_err(fail)? {
                let Ok(session) = sessions.GetSession(i) else {
                    continue;
                };
                let Ok(session) = session.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                if session.GetProcessId().is_ok_and(|pid| pid == me) {
                    continue;
                }
                let Ok(volume) = session.cast::<ISimpleAudioVolume>() else {
                    continue;
                };
                // Muted already (by the user): left alone, and left muted after.
                if volume.GetMute().is_ok_and(|m| m.as_bool()) {
                    continue;
                }
                if volume.SetMute(true, std::ptr::null()).is_ok() {
                    muted.push(volume);
                }
            }
        }
        Ok(SystemMute { muted })
    }

    pub fn stop(mute: &mut SystemMute) {
        for volume in mute.muted.drain(..) {
            unsafe {
                let _ = volume.SetMute(false, std::ptr::null());
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use super::SystemMute;

    pub fn start() -> Result<SystemMute, String> {
        Err("muting the other apps isn't supported here yet".into())
    }

    pub fn stop(_: &mut SystemMute) {}
}
