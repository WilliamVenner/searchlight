//! Networking utilities and abstractions

use std::{
	collections::BTreeSet,
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
	num::NonZeroU32,
};

/// The [`if_addrs`](https://crates.io/crates/if_addrs) crate is used to discover network interfaces on the system.
///
/// Here is a re-export for your convenience.
pub use if_addrs;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A wrapper around a raw IPv6 interface index.
///
/// With IPv6, interfaces are identified by their index, which is a number that is
/// guaranteed to be unique for the lifetime of the system.
///
/// This type provides a safe and idiomatic API for working with IPv6 interface indexes.
pub struct Ipv6Interface(pub NonZeroU32);
impl Ipv6Interface {
	/// Attempts to resolve the interface index from the given interface name.
	pub fn from_name(name: &str) -> Result<Self, std::io::Error> {
		Ok(Self(crate::util::iface_v6_name_to_index(name)?))
	}

	/// Attempts to resolve the interface index from the given interface address.
	pub fn from_addr(addr: &Ipv6Addr) -> Result<Self, std::io::Error> {
		if_addrs::get_if_addrs()?
			.into_iter()
			.find_map(|iface| {
				if let IpAddr::V6(iface_addr) = iface.ip() {
					if iface_addr == *addr {
						return Self::from_name(&iface.name).ok();
					}
				}
				None
			})
			.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Interface not found"))
	}

	/// Returns the IPv6 addresses of the interface.
	pub fn addrs(&self) -> Result<Vec<Ipv6Addr>, std::io::Error> {
		Ok(if_addrs::get_if_addrs()?
			.into_iter()
			.filter_map(|iface| {
				if let IpAddr::V6(addr) = iface.ip() {
					if Ipv6Interface::from_name(&iface.name).ok()? == *self {
						return Some(addr);
					}
				}
				None
			})
			.collect())
	}

	/// Returns the name of the interface.
	pub fn name(&self) -> Result<String, std::io::Error> {
		if_addrs::get_if_addrs()?
			.into_iter()
			.find_map(|iface| {
				if iface.ip().is_ipv6() && Ipv6Interface::from_name(&iface.name).ok()? == *self {
					Some(iface.name)
				} else {
					None
				}
			})
			.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Interface not found"))
	}

	#[inline(always)]
	/// Creates a new `Ipv6Interface` from the given raw interface index.
	pub fn from_raw(raw: NonZeroU32) -> Self {
		Self(raw)
	}

	#[inline(always)]
	/// Returns the raw interface index.
	///
	/// This will always be a non-zero value.
	pub fn as_u32(&self) -> u32 {
		self.0.get()
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The version of IP to use.
pub enum IpVersion {
	/// Use IPv4.
	V4,

	/// Use IPv6.
	V6,

	/// Use both IPv4 and IPv6.
	Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// The interface to use for multicast.
pub enum TargetInterface<Addr> {
	/// Let the OS decide which interface to use.
	Default,

	/// Use as many interfaces as possible, falling back to `Default` if none are available.
	All,

	/// Use the given interface.
	Specific(Addr),

	/// Use the given interfaces.
	Multi(BTreeSet<Addr>),
}

/// A `TargetInterface` for IPv4.
pub type TargetInterfaceV4 = TargetInterface<Ipv4Addr>;

/// A `TargetInterface` for IPv6.
pub type TargetInterfaceV6 = TargetInterface<Ipv6Interface>;

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
			let iface = libc::in_addr {
				s_addr: u32::from(iface).to_be(),
			};
			let res = libc::setsockopt(
				self.as_raw_fd(),
				libc::IPPROTO_IP,
				libc::IP_MULTICAST_IF,
				&iface as *const _ as *const _,
				std::mem::size_of::<libc::in_addr>() as libc::socklen_t,
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
