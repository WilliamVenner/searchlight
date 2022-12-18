use std::num::NonZeroU32;
use trust_dns_client::{
    proto::error::ProtoResult,
    rr::{IntoName, Name as DnsName},
};

pub fn iface_v6_name_to_index(name: &str) -> Result<NonZeroU32, std::io::Error> {
    use std::ffi::CString;

    #[cfg(windows)]
    use winapi::shared::netioapi::if_nametoindex;

    #[cfg(not(windows))]
    use libc::if_nametoindex;

    let name = CString::new(name).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid interface name")
    })?;
    let index = unsafe { if_nametoindex(name.as_ptr()) };
    NonZeroU32::new(index).ok_or_else(std::io::Error::last_os_error)
}

pub trait IntoDnsName: IntoName {
    fn into_fqdn(self) -> ProtoResult<DnsName> {
        let name = self.into_name()?;
        if !name.is_fqdn() {
            // Attempt to append the root label
            return name.append_name(&".".into_name()?);
        }
        Ok(name)
    }
}
impl<T: IntoName> IntoDnsName for T {}
