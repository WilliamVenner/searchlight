use std::{
	collections::BTreeSet,
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
	num::NonZeroU32,
};

pub use if_addrs;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ipv6Interface(pub NonZeroU32);
impl Ipv6Interface {
	fn metadata(&self) -> Result<(Ipv6Addr, String), std::io::Error> {
		if_addrs::get_if_addrs()?
			.into_iter()
			.find_map(|iface| {
				if let IpAddr::V6(addr) = iface.ip() {
					if Ipv6Interface::from_name(&iface.name).ok()? == *self {
						return Some((addr, iface.name));
					}
				}
				None
			})
			.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Interface not found"))
	}

	pub fn from_name(name: &str) -> Result<Self, std::io::Error> {
		Ok(Self(crate::util::iface_v6_name_to_index(name)?))
	}

	pub fn addr(&self) -> Result<Ipv6Addr, std::io::Error> {
		Ok(self.metadata()?.0)
	}

	pub fn name(&self) -> Result<String, std::io::Error> {
		Ok(self.metadata()?.1)
	}

	#[inline(always)]
	pub fn from_raw(raw: NonZeroU32) -> Self {
		Self(raw)
	}

	#[inline(always)]
	pub fn as_u32(&self) -> u32 {
		self.0.get()
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpVersion {
	V4,
	V6,
	Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetInterfaceV4 {
	Default,
	All,
	Specific(Ipv4Addr),
	Multi(BTreeSet<Ipv4Addr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetInterfaceV6 {
	Default,
	All,
	Specific(Ipv6Interface),
	Multi(BTreeSet<Ipv6Interface>),
}

pub(crate) trait MulticastSocketEx<Iface> {
	fn set_multicast_if(&self, iface: Iface) -> Result<(), std::io::Error>;
}

#[cfg(unix)]
impl MulticastSocketEx<Ipv6Interface> for tokio::net::UdpSocket {
	fn set_multicast_if(&self, iface: Ipv6Interface) -> Result<(), std::io::Error> {
		use std::os::unix::io::AsRawFd;
		unsafe {
			let res = libc::setsockopt(
				self.as_raw_fd(),
				libc::IPPROTO_IPV6,
				libc::IPV6_MULTICAST_IF,
				&iface.as_u32() as *const _ as *const _,
				std::mem::size_of::<u32>() as libc::socklen_t,
			);
			if res == 0 {
				Ok(())
			} else {
				Err(std::io::Error::last_os_error())
			}
		}
	}
}

#[cfg(unix)]
impl MulticastSocketEx<Ipv4Addr> for tokio::net::UdpSocket {
	fn set_multicast_if(&self, iface: Ipv4Addr) -> Result<(), std::io::Error> {
		use std::os::unix::io::AsRawFd;
		unsafe {
			let res = libc::setsockopt(
				self.as_raw_fd(),
				libc::IPPROTO_IP,
				libc::IP_MULTICAST_IF,
				&u32::from(iface) as *const _ as *const _,
				std::mem::size_of::<u32>() as libc::socklen_t,
			);
			if res == 0 {
				Ok(())
			} else {
				Err(std::io::Error::last_os_error())
			}
		}
	}
}

#[cfg(windows)]
impl MulticastSocketEx<Ipv6Interface> for tokio::net::UdpSocket {
	fn set_multicast_if(&self, iface: Ipv6Interface) -> Result<(), std::io::Error> {
		use std::os::windows::io::AsRawSocket;
		unsafe {
			let res = libc::setsockopt(
				self.as_raw_socket() as _,
				winapi::shared::ws2def::IPPROTO_IPV6 as _,
				winapi::shared::ws2ipdef::IPV6_MULTICAST_IF as _,
				&iface.as_u32() as *const _ as *const _,
				std::mem::size_of::<u32>() as _,
			);
			if res == 0 {
				Ok(())
			} else {
				Err(std::io::Error::last_os_error())
			}
		}
	}
}

#[cfg(windows)]
impl MulticastSocketEx<Ipv4Addr> for tokio::net::UdpSocket {
	fn set_multicast_if(&self, iface: Ipv4Addr) -> Result<(), std::io::Error> {
		let iface = u32::from_ne_bytes(iface.octets());

		use std::os::windows::io::AsRawSocket;
		unsafe {
			let res = libc::setsockopt(
				self.as_raw_socket() as _,
				winapi::shared::ws2def::IPPROTO_IP as _,
				winapi::shared::ws2ipdef::IP_MULTICAST_IF as _,
				&iface as *const _ as *const _,
				std::mem::size_of::<u32>() as _,
			);
			if res == 0 {
				Ok(())
			} else {
				Err(std::io::Error::last_os_error())
			}
		}
	}
}
