//! Bonjour advertising of `_nofriction._tcp` through the system's
//! mDNSResponder (`DNSServiceRegister`, part of libSystem). The record
//! lives while the returned [`Registration`] does. Same network only.

pub const SERVICE_TYPE: &str = "_nofriction._tcp";

/// TXT record: length-prefixed `key=value` strings
pub fn txt_record(mac_id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in [format!("id={}", mac_id), "v=1".to_string()] {
        let bytes = entry.as_bytes();
        out.push(bytes.len().min(255) as u8);
        out.extend_from_slice(&bytes[..bytes.len().min(255)]);
    }
    out
}

pub struct Registration {
    #[cfg(target_os = "macos")]
    sd_ref: *mut std::ffi::c_void,
}

// The DNSServiceRef is only touched on creation and drop
unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

#[cfg(target_os = "macos")]
mod ffi {
    use std::ffi::{c_char, c_void};
    extern "C" {
        #[allow(clippy::too_many_arguments)]
        pub fn DNSServiceRegister(
            sd_ref: *mut *mut c_void,
            flags: u32,
            interface_index: u32,
            name: *const c_char,
            regtype: *const c_char,
            domain: *const c_char,
            host: *const c_char,
            port_network_order: u16,
            txt_len: u16,
            txt_record: *const c_void,
            callback: *const c_void,
            context: *mut c_void,
        ) -> i32;
        pub fn DNSServiceRefDeallocate(sd_ref: *mut c_void);
    }
}

/// Advertise this Mac (the service name is the computer name).
pub fn register(port: u16, mac_id: &str) -> Result<Registration, String> {
    #[cfg(target_os = "macos")]
    {
        let regtype = std::ffi::CString::new(SERVICE_TYPE).expect("static");
        let txt = txt_record(mac_id);
        let mut sd_ref: *mut std::ffi::c_void = std::ptr::null_mut();
        let rc = unsafe {
            ffi::DNSServiceRegister(
                &mut sd_ref,
                0,
                0,
                std::ptr::null(),
                regtype.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                port.to_be(),
                txt.len() as u16,
                txt.as_ptr() as *const std::ffi::c_void,
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 || sd_ref.is_null() {
            return Err(format!("Bonjour registration failed ({})", rc));
        }
        Ok(Registration { sd_ref })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (port, mac_id);
        Ok(Registration {})
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unsafe {
            if !self.sd_ref.is_null() {
                ffi::DNSServiceRefDeallocate(self.sd_ref);
            }
        }
    }
}
