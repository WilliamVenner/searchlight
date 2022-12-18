use std::num::NonZeroU32;

pub fn iface_v6_name_to_index(name: &str) -> Option<NonZeroU32> {
    use std::ffi::CString;

    #[cfg(windows)]
    use winapi::shared::netioapi::if_nametoindex;

    #[cfg(not(windows))]
    extern "C" {
        fn if_nametoindex(ifname: *const std::ffi::c_char) -> u32;
    }

    let name = CString::new(name).ok()?;
    let index = unsafe { if_nametoindex(name.as_ptr()) };
    NonZeroU32::new(index)
}
