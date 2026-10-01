//! Core Audio queries (m4). Replaces the old `sh -c "ioreg -l | grep …"`
//! probe, which spawned a shell (not allowed in the App Sandbox) and matched
//! any audio engine, so it reported "active" almost all the time.
//!
//! Uses `kAudioDevicePropertyDeviceIsRunningSomewhere`: true while any
//! process (Zoom, Meet in a browser, …) has IO running on the device. The
//! CoreAudio framework is already linked by build.rs.

/// One client process of the audio server (macOS 14.2+ process objects).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioProcess {
    pub pid: i32,
    /// Empty when the process has none (daemons, some helpers).
    pub bundle_id: String,
    /// The process is running IO with at least one active input stream —
    /// i.e. it is using a microphone right now.
    pub running_input: bool,
    pub running_output: bool,
}

/// Which kinds of audio devices currently have IO running in some process.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioActivity {
    pub input: bool,
    pub output: bool,
}

#[cfg(target_os = "macos")]
mod ffi {
    use std::os::raw::c_void;

    pub type AudioObjectID = u32;
    pub type OSStatus = i32;

    #[repr(C)]
    pub struct AudioObjectPropertyAddress {
        pub selector: u32,
        pub scope: u32,
        pub element: u32,
    }

    const fn fourcc(s: &[u8; 4]) -> u32 {
        ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
    }

    pub const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;
    pub const K_AUDIO_HARDWARE_PROPERTY_DEVICES: u32 = fourcc(b"dev#");
    pub const K_AUDIO_DEVICE_PROPERTY_STREAMS: u32 = fourcc(b"stm#");
    pub const K_AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
    pub const K_SCOPE_GLOBAL: u32 = fourcc(b"glob");
    pub const K_SCOPE_INPUT: u32 = fourcc(b"inpt");
    pub const K_SCOPE_OUTPUT: u32 = fourcc(b"outp");
    pub const K_ELEMENT_MAIN: u32 = 0;
    // Process objects (macOS 14.2+; feature-detected at runtime)
    pub const K_AUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST: u32 = fourcc(b"prs#");
    pub const K_AUDIO_PROCESS_PROPERTY_PID: u32 = fourcc(b"ppid");
    pub const K_AUDIO_PROCESS_PROPERTY_BUNDLE_ID: u32 = fourcc(b"pbid");
    pub const K_AUDIO_PROCESS_PROPERTY_IS_RUNNING: u32 = fourcc(b"pir?");
    pub const K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_INPUT: u32 = fourcc(b"piri");
    pub const K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_OUTPUT: u32 = fourcc(b"piro");
    pub const K_AUDIO_PROCESS_PROPERTY_DEVICES: u32 = fourcc(b"pdv#");

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        pub fn AudioObjectHasProperty(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
        ) -> u8;
        pub fn AudioObjectGetPropertyDataSize(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            out_size: *mut u32,
        ) -> OSStatus;
        pub fn AudioObjectGetPropertyData(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            io_size: *mut u32,
            out_data: *mut c_void,
        ) -> OSStatus;
    }
}

#[cfg(target_os = "macos")]
fn device_ids() -> Vec<u32> {
    use ffi::*;
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_HARDWARE_PROPERTY_DEVICES,
        scope: K_SCOPE_GLOBAL,
        element: K_ELEMENT_MAIN,
    };
    unsafe {
        let mut size: u32 = 0;
        if AudioObjectGetPropertyDataSize(K_AUDIO_OBJECT_SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size) != 0
            || size == 0
        {
            return Vec::new();
        }
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids = vec![0u32; count];
        if AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr() as *mut _,
        ) != 0
        {
            return Vec::new();
        }
        ids.truncate(size as usize / std::mem::size_of::<AudioObjectID>());
        ids
    }
}

#[cfg(target_os = "macos")]
fn has_streams(device: u32, scope: u32) -> bool {
    use ffi::*;
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_DEVICE_PROPERTY_STREAMS,
        scope,
        element: K_ELEMENT_MAIN,
    };
    let mut size: u32 = 0;
    unsafe { AudioObjectGetPropertyDataSize(device, &addr, 0, std::ptr::null(), &mut size) == 0 && size > 0 }
}

#[cfg(target_os = "macos")]
fn is_running_somewhere(device: u32) -> bool {
    use ffi::*;
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE,
        scope: K_SCOPE_GLOBAL,
        element: K_ELEMENT_MAIN,
    };
    let mut running: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut running as *mut u32 as *mut _,
        )
    };
    status == 0 && running != 0
}

/// Input/output devices with IO running in any process (including ours).
#[cfg(target_os = "macos")]
pub fn audio_activity() -> AudioActivity {
    let mut out = AudioActivity::default();
    for id in device_ids() {
        if !is_running_somewhere(id) {
            continue;
        }
        if has_streams(id, ffi::K_SCOPE_INPUT) {
            out.input = true;
        }
        if has_streams(id, ffi::K_SCOPE_OUTPUT) {
            out.output = true;
        }
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn audio_activity() -> AudioActivity {
    AudioActivity::default()
}

#[cfg(target_os = "macos")]
fn has_property(object: u32, selector: u32, scope: u32) -> bool {
    let addr = ffi::AudioObjectPropertyAddress { selector, scope, element: ffi::K_ELEMENT_MAIN };
    unsafe { ffi::AudioObjectHasProperty(object, &addr) != 0 }
}

#[cfg(target_os = "macos")]
fn get_u32(object: u32, selector: u32) -> Option<u32> {
    use ffi::*;
    let addr = AudioObjectPropertyAddress { selector, scope: K_SCOPE_GLOBAL, element: K_ELEMENT_MAIN };
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(object, &addr, 0, std::ptr::null(), &mut size, &mut value as *mut u32 as *mut _)
    };
    (status == 0).then_some(value)
}

#[cfg(target_os = "macos")]
fn get_bundle_id(object: u32) -> String {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use ffi::*;
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_PROCESS_PROPERTY_BUNDLE_ID,
        scope: K_SCOPE_GLOBAL,
        element: K_ELEMENT_MAIN,
    };
    let mut cf: CFStringRef = std::ptr::null();
    let mut size = std::mem::size_of::<CFStringRef>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(object, &addr, 0, std::ptr::null(), &mut size, &mut cf as *mut CFStringRef as *mut _)
    };
    if status != 0 || cf.is_null() {
        return String::new();
    }
    // The caller owns the returned CFString (create rule)
    unsafe { CFString::wrap_under_create_rule(cf) }.to_string()
}

/// Does the process have an input device in its device list? Fallback for
/// systems that have process objects but not `IsRunningInput`.
#[cfg(target_os = "macos")]
fn process_has_input_device(object: u32) -> bool {
    use ffi::*;
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_PROCESS_PROPERTY_DEVICES,
        scope: K_SCOPE_INPUT,
        element: K_ELEMENT_MAIN,
    };
    let mut size: u32 = 0;
    unsafe { AudioObjectGetPropertyDataSize(object, &addr, 0, std::ptr::null(), &mut size) == 0 && size > 0 }
}

/// Every audio client process and whether it is using input/output right
/// now. None when the OS has no process objects (macOS < 14.2) — callers
/// must treat that as "signal unavailable", not "nobody is using the mic".
/// Read-only HAL property queries: no entitlement beyond what the app
/// already has, and no shell-outs (App Sandbox safe).
#[cfg(target_os = "macos")]
pub fn audio_processes() -> Option<Vec<AudioProcess>> {
    use ffi::*;
    if !has_property(K_AUDIO_OBJECT_SYSTEM_OBJECT, K_AUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST, K_SCOPE_GLOBAL) {
        return None;
    }
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST,
        scope: K_SCOPE_GLOBAL,
        element: K_ELEMENT_MAIN,
    };
    let ids: Vec<u32> = unsafe {
        let mut size: u32 = 0;
        if AudioObjectGetPropertyDataSize(K_AUDIO_OBJECT_SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size) != 0 {
            return None;
        }
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids = vec![0u32; count];
        if count > 0
            && AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                ids.as_mut_ptr() as *mut _,
            ) != 0
        {
            return None;
        }
        ids.truncate(size as usize / std::mem::size_of::<AudioObjectID>());
        ids
    };

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let pid = get_u32(id, K_AUDIO_PROCESS_PROPERTY_PID).map(|p| p as i32).unwrap_or(-1);
        let bundle_id = get_bundle_id(id);
        let (running_input, running_output) =
            if has_property(id, K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_INPUT, K_SCOPE_GLOBAL) {
                (
                    get_u32(id, K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_INPUT).unwrap_or(0) != 0,
                    get_u32(id, K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_OUTPUT).unwrap_or(0) != 0,
                )
            } else {
                // Older 14.x: running at all + has an input device
                let running = get_u32(id, K_AUDIO_PROCESS_PROPERTY_IS_RUNNING).unwrap_or(0) != 0;
                (running && process_has_input_device(id), running)
            };
        out.push(AudioProcess { pid, bundle_id, running_input, running_output });
    }
    Some(out)
}

#[cfg(not(target_os = "macos"))]
pub fn audio_processes() -> Option<Vec<AudioProcess>> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn query_does_not_crash() {
        // Values depend on the machine; this only exercises the FFI.
        let _ = super::audio_activity();
        let _ = super::audio_processes();
    }

    /// Smoke test against the real HAL: prints which processes are using
    /// audio input right now. `cargo test --lib core_audio -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_input_processes() {
        match super::audio_processes() {
            None => println!("process objects unavailable (macOS < 14.2?)"),
            Some(list) => {
                println!("{} audio client processes", list.len());
                for p in &list {
                    if p.running_input || p.running_output {
                        println!(
                            "  pid {:>6}  in={} out={}  {}  → call app: {:?}",
                            p.pid,
                            p.running_input,
                            p.running_output,
                            if p.bundle_id.is_empty() { "<no bundle id>" } else { &p.bundle_id },
                            crate::meeting_end::call_app_name(&p.bundle_id)
                        );
                    }
                }
                let ours = std::process::id() as i32;
                let active = crate::meeting_end::active_call_apps(&list, ours);
                println!("call apps using the mic (excluding us): {:?}", active);
            }
        }
    }
}
