use crate::{util::iface_v6_name_to_index, MDNS_PORT, MDNS_V4_IP, MDNS_V6_IP};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4},
    num::NonZeroU32,
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio::net::UdpSocket as AsyncUdpSocket;

pub(crate) type AsyncMdnsSocket = MdnsSocket<AsyncUdpSocket>;

pub(crate) enum MdnsSocket<S = std::net::UdpSocket> {
    V4(S),
    V6(MultiInterfaceIpv6Socket<S>),
    Multicol {
        v4: S,
        v6: MultiInterfaceIpv6Socket<S>,
    },
}
impl MdnsSocket<std::net::UdpSocket> {
    pub fn new(
        interface_v4: Option<Ipv4Addr>,
        interface_v6: Option<u32>,
    ) -> Result<Self, std::io::Error> {
        Ok(Self::Multicol {
            v4: match Self::new_v4(interface_v4)? {
                Self::V4(socket) => socket,
                _ => unreachable!(),
            },
            v6: match Self::new_v6(interface_v6)? {
                Self::V6(socket) => socket,
                _ => unreachable!(),
            },
        })
    }

    pub fn new_v4(interface: Option<Ipv4Addr>) -> Result<Self, std::io::Error> {
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )?;
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.set_reuse_address(true)?;
        socket.set_multicast_loop_v4(false)?;

        #[cfg(unix)]
        {
            socket.set_reuse_port(true)?;
        }

        if let Some(interface) = interface {
            socket.join_multicast_v4(&MDNS_V4_IP, &interface)?;
        } else {
            // Join multicast on all interfaces
            let mut did_join = false;
            if let Ok(ifaces) = if_addrs::get_if_addrs() {
                let mut joined = BTreeSet::new();
                for iface in ifaces
                    .into_iter()
                    .filter(|iface| !iface.is_loopback())
                    .filter_map(|iface| {
                        if let IpAddr::V4(iface) = iface.addr.ip() {
                            Some(iface)
                        } else {
                            None
                        }
                    })
                    .filter(|iface| joined.insert(*iface))
                {
                    if socket.join_multicast_v4(&MDNS_V4_IP, &iface).is_ok() {
                        did_join = true;
                    }
                }
            }
            if !did_join {
                socket.join_multicast_v4(&MDNS_V4_IP, &Ipv4Addr::UNSPECIFIED)?;
            }
        }

        socket.bind(&socket2::SockAddr::from(SocketAddr::new(
            IpAddr::V4(interface.unwrap_or(Ipv4Addr::UNSPECIFIED)),
            MDNS_PORT,
        )))?;

        socket.set_nonblocking(true)?;

        Ok(Self::V4(socket.into()))
    }

    pub fn new_v6(interface: Option<u32>) -> Result<Self, std::io::Error> {
        let mut resolved_interface = None;

        let socket = socket2::Socket::new(
            socket2::Domain::IPV6,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )?;
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.set_reuse_address(true)?;
        socket.set_only_v6(true)?;
        socket.set_multicast_loop_v6(false)?;

        #[cfg(unix)]
        {
            socket.set_reuse_port(true)?;
        }

        // TODO refactor in resolved_interface = Some case
        let ifaces = if_addrs::get_if_addrs()
            .map(|ifaces| {
                ifaces
                    .into_iter()
                    .filter(|iface| !iface.is_loopback())
                    .filter_map(|iface| {
                        let index = iface_v6_name_to_index(&iface.name).ok()?.get();

                        if socket.set_multicast_if_v6(index).is_err() {
                            return None;
                        }

                        if let Some(interface) = interface {
                            if index == interface {
                                resolved_interface = Some(iface.addr.ip());
                            }
                        }

                        if iface.addr.ip().is_ipv6() {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();

        if let Some(interface) = interface {
            socket.join_multicast_v6(&MDNS_V6_IP, interface)?;
        } else {
            // Join multicast on all interfaces
            let mut did_join = false;
            for iface in ifaces.iter().copied() {
                if socket.join_multicast_v6(&MDNS_V6_IP, iface).is_ok() {
                    did_join = true;
                }
            }
            if !did_join {
                socket.join_multicast_v6(&MDNS_V6_IP, 0)?;
            }
        }

        socket.bind(&socket2::SockAddr::from(SocketAddr::new(
            resolved_interface.unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            MDNS_PORT,
        )))?;

        socket.set_nonblocking(true)?;

        Ok(Self::V6(MultiInterfaceIpv6Socket {
            socket: socket.into(),
            ifaces,
        }))
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
    pub async fn send_multicast(&self, packet: &[u8]) -> Result<(), std::io::Error> {
        match self {
            Self::V4(v4) => v4
                .send_to(packet, SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT))
                .await
                .map(|_| ()),

            Self::V6(v6) => v6.send_multicast(packet).await.map(|_| ()),

            Self::Multicol { v4, v6 } => {
                let v4 = v4.send_to(packet, SocketAddrV4::new(MDNS_V4_IP, MDNS_PORT));
                let v6 = v6.send_multicast(packet);
                tokio::try_join!(v4, v6).map(|_| ())
            }
        }
    }

    pub fn recv(&self, buffer: Vec<u8>) -> MdnsSocketRecv {
        match self {
            Self::V4(socket) | Self::V6(MultiInterfaceIpv6Socket { socket, .. }) => {
                MdnsSocketRecv::Unicol(socket, buffer)
            }

            Self::Multicol {
                v4,
                v6: MultiInterfaceIpv6Socket { socket: v6, .. },
            } => MdnsSocketRecv::Multicol {
                v4: (v4, buffer.clone()),
                v6: (v6, buffer),
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

pub(crate) struct MultiInterfaceIpv6Socket<S = std::net::UdpSocket> {
    socket: S,
    ifaces: BTreeSet<u32>,
}
impl MultiInterfaceIpv6Socket {
    async fn into_async(self) -> Result<MultiInterfaceIpv6Socket<AsyncUdpSocket>, std::io::Error> {
        Ok(MultiInterfaceIpv6Socket {
            socket: AsyncUdpSocket::from_std(self.socket)?,
            ifaces: self.ifaces,
        })
    }
}
impl MultiInterfaceIpv6Socket<AsyncUdpSocket> {
    pub async fn send_multicast(&self, packet: &[u8]) -> Result<(), std::io::Error> {
        for iface in self.ifaces.iter() {
            unsafe {
                let res = {
                    #[cfg(unix)]
                    {
                        use std::os::unix::io::AsRawFd;
                        libc::setsockopt(
                            self.socket.as_raw_fd(),
                            libc::IPPROTO_IPV6,
                            libc::IPV6_MULTICAST_IF,
                            iface as *const _ as *const _,
                            std::mem::size_of::<u32>() as libc::socklen_t,
                        )
                    }
                    #[cfg(windows)]
                    {
                        use std::os::windows::io::AsRawHandle;
                        libc::setsockopt(
                            self.socket.as_raw_handle(),
                            libc::IPPROTO_IPV6,
                            libc::IPV6_MULTICAST_IF,
                            iface as *const _ as *const _,
                            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                        )
                    }
                };
                if res != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }

            self.socket
                .send_to(packet, SocketAddr::new(IpAddr::V6(MDNS_V6_IP), MDNS_PORT))
                .await?;
        }
        Ok(())
    }
}
