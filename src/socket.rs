use crate::{
	errors::MultiIpIoError,
	net::{Ipv6Interface, MulticastSocketEx, TargetInterfaceV4, TargetInterfaceV6},
	util::iface_v6_name_to_index,
	MDNS_PORT, MDNS_V4_IP, MDNS_V6_IP,
};
use std::{
	collections::BTreeSet,
	net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, UdpSocket},
	time::Duration,
};
use tokio::net::{ToSocketAddrs, UdpSocket as AsyncUdpSocket};

pub(crate) type AsyncMdnsSocket = MdnsSocket<AsyncUdpSocket>;
pub(crate) enum MdnsSocket<Socket = UdpSocket> {
	V4(InterfacedMdnsSocket<Socket, Ipv4Addr>),
	V6(InterfacedMdnsSocket<Socket, Ipv6Interface>),
	Multicol {
		v4: InterfacedMdnsSocket<Socket, Ipv4Addr>,
		v6: InterfacedMdnsSocket<Socket, Ipv6Interface>,
	},
}
impl MdnsSocket<UdpSocket> {
	pub fn new(loopback: bool, interface_v4: TargetInterfaceV4, interface_v6: TargetInterfaceV6) -> Result<Self, (std::io::Error, std::io::Error)> {
		let v4 = Self::new_v4(loopback, interface_v4).map(|socket| match socket {
			MdnsSocket::V4(socket) => socket,
			_ => unreachable!(),
		});

		let v6 = Self::new_v6(loopback, interface_v6).map(|socket| match socket {
			MdnsSocket::V6(socket) => socket,
			_ => unreachable!(),
		});

		match (v4, v6) {
			(Ok(v4), Ok(v6)) => Ok(Self::Multicol { v4, v6 }),
			(Err(v4), Err(v6)) => Err((v4, v6)),
			(Ok(v4), Err(_)) => Ok(MdnsSocket::V4(v4)),
			(Err(_), Ok(v6)) => Ok(MdnsSocket::V6(v6)),
		}
	}

	pub fn new_v4(loopback: bool, interface: TargetInterfaceV4) -> Result<Self, std::io::Error> {
		let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
		socket.set_read_timeout(Some(Duration::from_millis(100)))?;
		socket.set_reuse_address(true)?;
		socket.set_multicast_loop_v4(loopback)?;

		#[cfg(unix)]
		{
			socket.set_reuse_port(true)?;
		}

		let ifaces = match interface {
			TargetInterfaceV4::Default => {
				socket.join_multicast_v4(&MDNS_V4_IP, &Ipv4Addr::UNSPECIFIED)?;

				BTreeSet::new()
			}

			TargetInterfaceV4::Specific(iface) => {
				socket.join_multicast_v4(&MDNS_V4_IP, &iface)?;

				BTreeSet::from_iter([iface])
			}

			TargetInterfaceV4::Multi(ifaces) => {
				for iface in ifaces.iter() {
					socket.join_multicast_v4(&MDNS_V4_IP, iface)?;
				}

				ifaces
			}

			TargetInterfaceV4::All => {
				let mut all_interfaces = if_addrs::get_if_addrs()
					.map(|ifaces| {
						ifaces
							.into_iter()
							.filter(|iface| !iface.is_loopback())
							.filter_map(|iface| if let IpAddr::V4(iface) = iface.addr.ip() { Some(iface) } else { None })
							.collect::<BTreeSet<Ipv4Addr>>()
					})
					.unwrap_or_default();

				let mut did_join = false;
				all_interfaces.retain(|iface| {
					if socket.set_multicast_if_v4(iface).is_ok() && socket.join_multicast_v4(&MDNS_V4_IP, iface).is_ok() {
						did_join = true;
						true
					} else {
						false
					}
				});
				if !did_join {
					// Fallback to default
					socket.join_multicast_v4(&MDNS_V4_IP, &Ipv4Addr::UNSPECIFIED)?;
				}

				all_interfaces
			}
		};

		socket.bind(&socket2::SockAddr::from(SocketAddr::new(
			IpAddr::V4(if cfg!(windows) && ifaces.len() == 1 {
				*ifaces.iter().next().unwrap()
			} else {
				Ipv4Addr::UNSPECIFIED
			}),
			MDNS_PORT,
		)))?;

		// Make sure the socket works
		socket.set_multicast_if_v4(&Ipv4Addr::UNSPECIFIED)?; // Set to default interface
		socket.send_to(&[0], &SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT).into())?; // Send a multicast packet

		// If we're only using one interface, set it as the default
		if ifaces.len() == 1 {
			let addr = ifaces.iter().next().unwrap();
			socket.set_multicast_if_v4(addr)?;
		}

		Ok(Self::V4(InterfacedMdnsSocket::new(socket.into(), ifaces)))
	}

	pub fn new_v6(loopback: bool, interface: TargetInterfaceV6) -> Result<Self, std::io::Error> {
		let socket = socket2::Socket::new(socket2::Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
		socket.set_read_timeout(Some(Duration::from_millis(100)))?;
		socket.set_reuse_address(true)?;
		socket.set_only_v6(true)?;
		socket.set_multicast_loop_v6(loopback)?;

		#[cfg(unix)]
		{
			socket.set_reuse_port(true)?;
		}

		let ifaces = match interface {
			TargetInterfaceV6::Default => {
				socket.join_multicast_v6(&MDNS_V6_IP, 0)?;

				BTreeSet::new()
			}

			TargetInterfaceV6::Specific(iface) => {
				socket.join_multicast_v6(&MDNS_V6_IP, iface.as_u32())?;

				BTreeSet::from_iter([iface])
			}

			TargetInterfaceV6::Multi(ifaces) => {
				for iface in ifaces.iter() {
					socket.join_multicast_v6(&MDNS_V6_IP, iface.as_u32())?;
				}

				ifaces
			}

			TargetInterfaceV6::All => {
				let mut all_interfaces = if_addrs::get_if_addrs()
					.map(|ifaces| {
						ifaces
							.into_iter()
							.filter(|iface| !iface.is_loopback() && iface.addr.ip().is_ipv6())
							.filter_map(|iface| iface_v6_name_to_index(&iface.name).ok().map(Ipv6Interface::from_raw))
							.collect::<BTreeSet<_>>()
					})
					.unwrap_or_default();

				let mut did_join = false;
				all_interfaces.retain(|iface| {
					if socket.set_multicast_if_v6(iface.as_u32()).is_ok() && socket.join_multicast_v6(&MDNS_V6_IP, iface.as_u32()).is_ok() {
						did_join = true;
						true
					} else {
						false
					}
				});
				if !did_join {
					// Fallback to default
					socket.join_multicast_v6(&MDNS_V6_IP, 0)?;
				}

				all_interfaces
			}
		};

		socket.bind(&socket2::SockAddr::from(SocketAddr::new(
			IpAddr::V6({
				let mut bind_addr = Ipv6Addr::UNSPECIFIED;
				if cfg!(windows) && ifaces.len() == 1 {
					let iface = ifaces.iter().next().unwrap();
					let addrs = iface.addrs()?;
					if addrs.len() == 1 {
						bind_addr = addrs.into_iter().next().unwrap();
					}
				}
				bind_addr
			}),
			MDNS_PORT,
		)))?;

		// Make sure the socket works
		socket.set_multicast_if_v6(0)?; // Set to default interface
		socket.send_to(&[0], &SocketAddr::new(IpAddr::V6(MDNS_V6_IP), MDNS_PORT).into())?; // Send a multicast packet

		// If we're only using one interface, set it as the default
		if ifaces.len() == 1 {
			let iface = ifaces.iter().next().unwrap();
			socket.set_multicast_if_v6(iface.as_u32())?;
		}

		Ok(Self::V6(InterfacedMdnsSocket::new(socket.into(), ifaces)))
	}

	pub async fn into_async(self) -> Result<AsyncMdnsSocket, MultiIpIoError> {
		Ok(match self {
			Self::V4(v4) => AsyncMdnsSocket::V4(v4.into_async().map_err(MultiIpIoError::V4)?),
			Self::V6(v6) => AsyncMdnsSocket::V6(v6.into_async().map_err(MultiIpIoError::V6)?),
			Self::Multicol { v4, v6 } => AsyncMdnsSocket::Multicol {
				v4: v4.into_async().map_err(MultiIpIoError::V4)?,
				v6: v6.into_async().map_err(MultiIpIoError::V6)?,
			},
		})
	}
}
impl AsyncMdnsSocket {
	pub async fn send_to(&self, packet: &[u8], addr: SocketAddr) -> Result<(), MultiIpIoError> {
		match (addr, self) {
			(SocketAddr::V4(addr), Self::V4(v4) | Self::Multicol { v4, .. }) => v4.send_to(packet, addr).await.map_err(MultiIpIoError::V4),
			(SocketAddr::V6(addr), Self::V6(v6) | Self::Multicol { v6, .. }) => v6.send_to(packet, addr).await.map_err(MultiIpIoError::V6),

			(SocketAddr::V6(_), Self::V4(_)) => Err(MultiIpIoError::V4(std::io::Error::new(
				std::io::ErrorKind::InvalidInput,
				"Invalid address (only IPv4 available, got IPv6 address)",
			))),

			(SocketAddr::V4(_), Self::V6(_)) => Err(MultiIpIoError::V4(std::io::Error::new(
				std::io::ErrorKind::InvalidInput,
				"Invalid address (only IPv6 available, got IPv4 address)",
			))),
		}
	}

	pub async fn send_multicast(&self, packet: &[u8]) -> Result<(), MultiIpIoError> {
		match self {
			Self::V4(v4) => v4
				.send_to_multicast(packet, SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT))
				.await
				.map_err(MultiIpIoError::V4),

			Self::V6(v6) => v6
				.send_to_multicast(packet, SocketAddr::new(IpAddr::V6(MDNS_V6_IP), MDNS_PORT))
				.await
				.map_err(MultiIpIoError::V6),

			Self::Multicol { v4, v6 } => {
				let v4 = v4.send_to_multicast(packet, SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT));
				let v6 = v6.send_to_multicast(packet, SocketAddr::new(IpAddr::V6(MDNS_V6_IP), MDNS_PORT));
				match tokio::join!(v4, v6) {
					(Ok(_), _) | (_, Ok(_)) => Ok(()),
					(Err(v4), Err(v6)) => Err(MultiIpIoError::Both { v4, v6 }),
				}
			}
		}
	}

	pub fn recv(&self, buffer: Vec<u8>) -> MdnsSocketRecv {
		match self {
			#[rustfmt::skip]
			Self::V4(InterfacedMdnsSocket::UniInterface(socket) | InterfacedMdnsSocket::MultiInterface { socket, .. }) => {
				MdnsSocketRecv::V4(socket, buffer)
			},

			Self::V6(InterfacedMdnsSocket::UniInterface(socket) | InterfacedMdnsSocket::MultiInterface { socket, .. }) => {
				MdnsSocketRecv::V6(socket, buffer)
			}

			Self::Multicol {
				v4: InterfacedMdnsSocket::UniInterface(v4) | InterfacedMdnsSocket::MultiInterface { socket: v4, .. },
				v6: InterfacedMdnsSocket::UniInterface(v6) | InterfacedMdnsSocket::MultiInterface { socket: v6, .. },
			} => MdnsSocketRecv::Multicol {
				v4: (v4, buffer.clone()),
				v6: (v6, buffer),
			},
		}
	}
}

pub enum MdnsSocketRecv<'a> {
	V4(&'a AsyncUdpSocket, Vec<u8>),
	V6(&'a AsyncUdpSocket, Vec<u8>),
	Multicol {
		v4: (&'a AsyncUdpSocket, Vec<u8>),
		v6: (&'a AsyncUdpSocket, Vec<u8>),
	},
}
impl MdnsSocketRecv<'_> {
	pub async fn recv_multicast(&mut self) -> Result<((usize, SocketAddr), &[u8]), MultiIpIoError> {
		match self {
			Self::V4(socket, buf) => Ok((socket.recv_from(buf).await.map_err(MultiIpIoError::V4)?, buf)),
			Self::V6(socket, buf) => Ok((socket.recv_from(buf).await.map_err(MultiIpIoError::V6)?, buf)),
			Self::Multicol {
				v4: (v4, buf_v4),
				v6: (v6, buf_v6),
			} => {
				let v4 = async { v4.recv_from(buf_v4).await.map(|recv| (recv, &**buf_v4)) };
				let v6 = async { v6.recv_from(buf_v6).await.map(|recv| (recv, &**buf_v6)) };
				tokio::pin!(v4);
				tokio::pin!(v6);
				tokio::select! {
					v4 = &mut v4 => match v4 {
						Ok(v4) => Ok(v4),

						Err(v4) => match v6.await {
							Ok(v6) => Ok(v6),
							Err(v6) => Err(MultiIpIoError::Both { v4, v6 })
						},
					},

					v6 = &mut v6 => match v6 {
						Ok(v6) => Ok(v6),

						Err(v6) => match v4.await {
							Ok(v4) => Ok(v4),
							Err(v4) => Err(MultiIpIoError::Both { v4, v6 })
						},
					}
				}
			}
		}
	}
}

pub(crate) enum InterfacedMdnsSocket<Socket, Iface>
where
	Iface: PartialEq + Eq + PartialOrd + Ord + Copy,
{
	UniInterface(Socket),
	MultiInterface { socket: Socket, ifaces: BTreeSet<Iface> },
}
impl<Socket, Iface> InterfacedMdnsSocket<Socket, Iface>
where
	Iface: PartialEq + Eq + PartialOrd + Ord + Copy,
{
	fn new(socket: Socket, ifaces: BTreeSet<Iface>) -> Self {
		match ifaces.len() {
			0 | 1 => Self::UniInterface(socket),
			_ => Self::MultiInterface { socket, ifaces },
		}
	}
}
impl<Iface> InterfacedMdnsSocket<UdpSocket, Iface>
where
	Iface: PartialEq + Eq + PartialOrd + Ord + Copy,
{
	fn into_async(self) -> Result<InterfacedMdnsSocket<AsyncUdpSocket, Iface>, std::io::Error> {
		Ok(match self {
			Self::UniInterface(socket) => {
				socket.set_nonblocking(true)?;
				InterfacedMdnsSocket::UniInterface(AsyncUdpSocket::from_std(socket)?)
			}

			Self::MultiInterface { socket, ifaces } => InterfacedMdnsSocket::MultiInterface {
				socket: {
					socket.set_nonblocking(true)?;
					AsyncUdpSocket::from_std(socket)?
				},

				ifaces,
			},
		})
	}
}
impl<Iface> InterfacedMdnsSocket<AsyncUdpSocket, Iface>
where
	AsyncUdpSocket: MulticastSocketEx<Iface>,
	Iface: PartialEq + Eq + PartialOrd + Ord + Copy + std::fmt::Debug,
{
	pub async fn send_to(&self, packet: &[u8], addr: impl ToSocketAddrs + Copy) -> Result<(), std::io::Error> {
		let socket = match self {
			Self::UniInterface(socket) => socket,
			Self::MultiInterface { socket, .. } => socket,
		};

		socket.send_to(packet, addr).await.map(|_| ())
	}

	pub async fn send_to_multicast(&self, packet: &[u8], multicast_addr: impl ToSocketAddrs + Copy) -> Result<(), std::io::Error> {
		match self {
			Self::UniInterface(socket) => {
				socket.send_to(packet, multicast_addr).await?;
			}

			Self::MultiInterface { socket, ifaces } => {
				debug_assert!(ifaces.len() > 1);

				for iface in ifaces.iter().copied() {
					socket.set_multicast_if(iface)?;
					socket.send_to(packet, multicast_addr).await?;
				}
			}
		}

		Ok(())
	}
}
