use crate::{util::iface_v6_name_to_index, MDNS_PORT, MDNS_V4_IP, MDNS_V6_IP};
use std::{
	collections::BTreeSet,
	net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4},
	num::NonZeroU32,
	time::Duration,
};
use tokio::net::{ToSocketAddrs, UdpSocket as AsyncUdpSocket};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpVersion {
	V4,
	V6,
	Both,
}

#[derive(Clone, Copy, Debug)]
pub enum TargetInterface<Addr> {
	Default,
	All,
	Specific(Addr),
}

pub(crate) type AsyncMdnsSocket = MdnsSocket<AsyncUdpSocket>;
pub(crate) enum MdnsSocket<S = std::net::UdpSocket> {
	V4(S),
	V6(Ipv6MdnsSocket<S>),
	Multicol { v4: S, v6: Ipv6MdnsSocket<S> },
}
impl MdnsSocket<std::net::UdpSocket> {
	pub fn new(loopback: bool, interface_v4: TargetInterface<Ipv4Addr>, interface_v6: TargetInterface<NonZeroU32>) -> Result<Self, std::io::Error> {
		Ok(Self::Multicol {
			v4: match Self::new_v4(loopback, interface_v4)? {
				Self::V4(socket) => socket,
				_ => unreachable!(),
			},
			v6: match Self::new_v6(loopback, interface_v6)? {
				Self::V6(socket) => socket,
				_ => unreachable!(),
			},
		})
	}

	pub fn new_v4(loopback: bool, interface: TargetInterface<Ipv4Addr>) -> Result<Self, std::io::Error> {
		let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
		socket.set_read_timeout(Some(Duration::from_millis(100)))?;
		socket.set_reuse_address(true)?;
		socket.set_multicast_loop_v4(loopback)?;

		#[cfg(unix)]
		{
			socket.set_reuse_port(true)?;
		}

		match interface {
			TargetInterface::Default => {
				socket.join_multicast_v4(&MDNS_V4_IP, &Ipv4Addr::UNSPECIFIED)?;
			}

			TargetInterface::Specific(iface) => {
				socket.join_multicast_v4(&MDNS_V4_IP, &iface)?;
			}

			TargetInterface::All => {
				let mut did_join = false;
				for iface in if_addrs::get_if_addrs()
					.map(|ifaces| {
						ifaces
							.into_iter()
							.filter(|iface| !iface.is_loopback())
							.filter_map(|iface| if let IpAddr::V4(iface) = iface.addr.ip() { Some(iface) } else { None })
							.collect::<BTreeSet<Ipv4Addr>>()
					})
					.unwrap_or_default()
				{
					if socket.join_multicast_v4(&MDNS_V4_IP, &iface).is_ok() {
						did_join = true;
					}
				}
				if !did_join {
					// Fallback to default
					socket.join_multicast_v4(&MDNS_V4_IP, &Ipv4Addr::UNSPECIFIED)?;
				}
			}
		}

		socket.bind(&socket2::SockAddr::from(SocketAddr::new(
			IpAddr::V4(if let TargetInterface::Specific(addr) = interface {
				addr
			} else {
				Ipv4Addr::UNSPECIFIED
			}),
			MDNS_PORT,
		)))?;

		socket.set_nonblocking(true)?;

		Ok(Self::V4(socket.into()))
	}

	pub fn new_v6(loopback: bool, interface: TargetInterface<NonZeroU32>) -> Result<Self, std::io::Error> {
		let socket = socket2::Socket::new(socket2::Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
		socket.set_read_timeout(Some(Duration::from_millis(100)))?;
		socket.set_reuse_address(true)?;
		socket.set_only_v6(true)?;
		socket.set_multicast_loop_v6(loopback)?;

		#[cfg(unix)]
		{
			socket.set_reuse_port(true)?;
		}

		// In the case of TargetInterface::Specific, we need to convert the index to an IP address for binding
		let mut resolved_interface = None;

		let all_interfaces = if_addrs::get_if_addrs()
			.map(|ifaces| {
				ifaces
					.into_iter()
					.filter(|iface| !iface.is_loopback() && iface.addr.ip().is_ipv6())
					.filter_map(|iface| {
						let index = iface_v6_name_to_index(&iface.name).ok()?.get();

						if let TargetInterface::Specific(unresolved_index) = interface {
							if unresolved_index.get() == index {
								resolved_interface = Some(iface.addr.ip());
							}
						}

						if socket.set_multicast_if_v6(index).is_err() {
							return None;
						}

						Some(index)
					})
					.collect::<BTreeSet<_>>()
			})
			.unwrap_or_default();

		match interface {
			TargetInterface::Default => {
				socket.join_multicast_v6(&MDNS_V6_IP, 0)?;
			}

			TargetInterface::Specific(index) => {
				socket.join_multicast_v6(&MDNS_V6_IP, index.get())?;
			}

			TargetInterface::All => {
				let mut did_join = false;

				for iface in all_interfaces.iter().copied() {
					if socket.join_multicast_v6(&MDNS_V6_IP, iface).is_ok() {
						did_join = true;
					}
				}

				if !did_join {
					// Fallback to default
					socket.join_multicast_v6(&MDNS_V6_IP, 0)?;
				}
			}
		}

		socket.bind(&socket2::SockAddr::from(SocketAddr::new(
			resolved_interface.unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
			MDNS_PORT,
		)))?;

		socket.set_nonblocking(true)?;

		Ok(Self::V6(Ipv6MdnsSocket::new(socket, all_interfaces)?))
	}

	pub async fn into_async(self) -> Result<AsyncMdnsSocket, std::io::Error> {
		Ok(match self {
			Self::V4(v4) => AsyncMdnsSocket::V4(AsyncUdpSocket::from_std(v4)?),
			Self::V6(v6) => AsyncMdnsSocket::V6(v6.into_async().await?),
			Self::Multicol { v4, v6 } => AsyncMdnsSocket::Multicol {
				v4: AsyncUdpSocket::from_std(v4)?,
				v6: v6.into_async().await?,
			},
		})
	}
}
impl AsyncMdnsSocket {
	pub async fn send_to(&self, packet: &[u8], addr: SocketAddr) -> Result<(), std::io::Error> {
		match (addr, self) {
			(SocketAddr::V4(addr), Self::V4(v4) | Self::Multicol { v4, .. }) => v4.send_to(packet, addr).await.map(|_| ()),

			(SocketAddr::V6(addr), Self::V6(v6) | Self::Multicol { v6, .. }) => v6.send_to_multicast(packet, addr).await.map(|_| ()),

			_ => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid address")),
		}
	}

	pub async fn send_multicast(&self, packet: &[u8]) -> Result<(), std::io::Error> {
		match self {
			Self::V4(v4) => v4.send_to(packet, SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT)).await.map(|_| ()),

			Self::V6(v6) => v6
				.send_to_multicast(packet, SocketAddr::new(IpAddr::V6(MDNS_V6_IP), MDNS_PORT))
				.await
				.map(|_| ()),

			Self::Multicol { v4, v6 } => {
				let v4 = v4.send_to(packet, SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT));
				let v6 = v6.send_to_multicast(packet, SocketAddr::new(IpAddr::V6(MDNS_V6_IP), MDNS_PORT));
				tokio::try_join!(v4, v6).map(|_| ())
			}
		}
	}

	pub fn recv(&self, buffer: Vec<u8>) -> MdnsSocketRecv {
		match self {
			Self::V4(socket) | Self::V6(Ipv6MdnsSocket::Single(socket) | Ipv6MdnsSocket::Multi { socket, .. }) => {
				MdnsSocketRecv::Unicol(socket, buffer)
			}

			Self::Multicol { v4, v6 } => MdnsSocketRecv::Multicol {
				v4: (v4, buffer.clone()),
				v6: (v6.socket(), buffer),
			},
		}
	}
}

pub enum MdnsSocketRecv<'a> {
	Unicol(&'a AsyncUdpSocket, Vec<u8>),
	Multicol {
		v4: (&'a AsyncUdpSocket, Vec<u8>),
		v6: (&'a AsyncUdpSocket, Vec<u8>),
	},
}
impl MdnsSocketRecv<'_> {
	pub async fn recv_multicast(&mut self) -> Result<((usize, SocketAddr), &[u8]), std::io::Error> {
		Ok(match self {
			Self::Unicol(socket, buf) => (socket.recv_from(buf).await?, buf),
			Self::Multicol {
				v4: (v4, buf_v4),
				v6: (v6, buf_v6),
			} => {
				let v4 = v4.recv_from(buf_v4);
				let v6 = v6.recv_from(buf_v6);
				tokio::select! {
					v4 = v4 => (v4?, buf_v4),
					v6 = v6 => (v6?, buf_v6),
				}
			}
		})
	}
}

/// Hacky abstraction that allows us to send to multicast on a group of interfaces
pub(crate) enum Ipv6MdnsSocket<S = std::net::UdpSocket> {
	Single(S),
	Multi { socket: S, ifaces: BTreeSet<u32> },
}
impl Ipv6MdnsSocket {
	fn new(socket: socket2::Socket, ifaces: BTreeSet<u32>) -> Result<Self, std::io::Error> {
		match ifaces.len() {
			0 => socket.set_multicast_if_v6(0)?,
			1 => socket.set_multicast_if_v6(ifaces.iter().copied().next().unwrap())?,
			_ => {
				return Ok(Self::Multi {
					socket: socket.into(),
					ifaces,
				})
			}
		}

		Ok(Self::Single(socket.into()))
	}

	async fn into_async(self) -> Result<Ipv6MdnsSocket<AsyncUdpSocket>, std::io::Error> {
		Ok(match self {
			Self::Single(socket) => Ipv6MdnsSocket::Single(AsyncUdpSocket::from_std(socket)?),
			Self::Multi { socket, ifaces } => Ipv6MdnsSocket::Multi {
				socket: AsyncUdpSocket::from_std(socket)?,
				ifaces,
			},
		})
	}
}
impl Ipv6MdnsSocket<AsyncUdpSocket> {
	pub async fn send_to_multicast(&self, packet: &[u8], addr: impl ToSocketAddrs + Copy) -> Result<(), std::io::Error> {
		match self {
			Self::Single(socket) => {
				socket.send_to(packet, addr).await?;
			}

			Self::Multi { socket, ifaces } => {
				debug_assert!(ifaces.len() > 1);

				for iface in ifaces.iter().copied() {
					socket.set_multicast_if_v6(iface)?;
					socket.send_to(packet, addr).await?;
				}
			}
		}

		Ok(())
	}

	fn socket(&self) -> &AsyncUdpSocket {
		match self {
			Self::Single(socket) => socket,
			Self::Multi { socket, .. } => socket,
		}
	}
}

trait SetIpv6MulticastInterface {
	fn set_multicast_if_v6(&self, iface: u32) -> Result<(), std::io::Error>;
}
impl SetIpv6MulticastInterface for AsyncUdpSocket {
	fn set_multicast_if_v6(&self, iface: u32) -> Result<(), std::io::Error> {
		let res = {
			#[cfg(unix)]
			{
				use std::os::unix::io::AsRawFd;
				unsafe {
					libc::setsockopt(
						self.as_raw_fd(),
						libc::IPPROTO_IPV6,
						libc::IPV6_MULTICAST_IF,
						&iface as *const _ as *const _,
						std::mem::size_of::<u32>() as libc::socklen_t,
					)
				}
			}
			#[cfg(windows)]
			{
				use std::os::windows::io::AsRawHandle;
				unsafe {
					libc::setsockopt(
						self.as_raw_handle(),
						libc::IPPROTO_IPV6,
						libc::IPV6_MULTICAST_IF,
						&iface as *const _ as *const _,
						std::mem::size_of::<libc::c_int>() as libc::socklen_t,
					)
				}
			}
		};
		if res == 0 {
			Ok(())
		} else {
			Err(std::io::Error::last_os_error())
		}
	}
}
