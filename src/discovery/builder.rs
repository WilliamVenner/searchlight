use super::Discovery;
use crate::{errors::BadNameError, socket::MdnsSocket, util::IntoDnsName};
use std::{net::Ipv4Addr, num::NonZeroU32, time::Duration};
use trust_dns_client::rr::Name as DnsName;

pub struct DiscoveryBuilder {
    service_name: Option<DnsName>,
    interval: Duration,
    responder_window: Duration,
    loopback: bool,
}
impl DiscoveryBuilder {
    pub fn new() -> Self {
        Self {
            service_name: None,
            interval: Duration::from_secs(10),
            responder_window: Duration::from_secs(10),
            loopback: false,
        }
    }

    pub fn service(mut self, service_name: impl IntoDnsName) -> Result<Self, BadNameError> {
        self.service_name = Some(service_name.into_fqdn().map_err(|_| BadNameError)?);
        Ok(self)
    }

    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self.responder_window = self.responder_window.max(interval);
        self
    }

    /// The responder window is the amount of time that a responder is considered active.
    ///
    /// If a responder goes quiet for this amount of time, it is considered "lost" and will emit a [`DiscoveryEvent::ResponderLost`](crate::discovery::event::DiscoveryEvent::ResponderLost) event.
    ///
    /// This value must be greater than or equal to the interval, it will be clamped to the interval if it is any less.
    pub fn responder_window(mut self, peer_window: Duration) -> Self {
        self.responder_window = peer_window.max(self.interval);
        self
    }

    pub fn loopback(mut self) -> Self {
        self.loopback = true;
        self
    }

    pub fn only_ipv4(self) -> DiscoveryInterfaceBuilderV4 {
        DiscoveryInterfaceBuilderV4 {
            builder: self,
            interface: None,
        }
    }

    pub fn only_ipv6(self) -> DiscoveryInterfaceBuilderV6 {
        DiscoveryInterfaceBuilderV6 {
            builder: self,
            interface: None,
        }
    }

    pub fn any_ip(self) -> DiscoveryInterfaceBuilderAny {
        DiscoveryInterfaceBuilderAny {
            builder: self,
            interface_v4: None,
            interface_v6: None,
        }
    }
}
impl Default for DiscoveryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DiscoveryInterfaceBuilderV4 {
    builder: DiscoveryBuilder,
    interface: Option<Ipv4Addr>,
}
impl DiscoveryInterfaceBuilderV4 {
    pub fn interface(mut self, interface: Ipv4Addr) -> Self {
        self.interface = Some(interface);
        self
    }

    pub fn all_interfaces(mut self) -> Self {
        self.interface = None;
        self
    }

    pub fn default_interface(mut self) -> Self {
        self.interface = Some(Ipv4Addr::UNSPECIFIED);
        self
    }

    pub fn build(self) -> Result<Discovery, std::io::Error> {
        let DiscoveryBuilder {
            loopback,
            service_name,
            interval,
            responder_window: peer_window,
        } = self.builder;

        Ok(Discovery {
            socket: MdnsSocket::new_v4(loopback, self.interface)?,
            service_name,
            interval,
            peer_window,
        })
    }
}

pub struct DiscoveryInterfaceBuilderV6 {
    builder: DiscoveryBuilder,
    interface: Option<u32>,
}
impl DiscoveryInterfaceBuilderV6 {
    pub fn interface(mut self, interface: NonZeroU32) -> Self {
        self.interface = Some(interface.get());
        self
    }

    pub fn all_interfaces(mut self) -> Self {
        self.interface = None;
        self
    }

    pub fn default_interface(mut self) -> Self {
        self.interface = Some(0);
        self
    }

    pub fn build(self) -> Result<Discovery, std::io::Error> {
        let DiscoveryBuilder {
            loopback,
            service_name,
            interval,
            responder_window: peer_window,
        } = self.builder;

        Ok(Discovery {
            socket: MdnsSocket::new_v6(loopback, self.interface)?,
            service_name,
            interval,
            peer_window,
        })
    }
}

pub struct DiscoveryInterfaceBuilderAny {
    builder: DiscoveryBuilder,
    interface_v4: Option<Ipv4Addr>,
    interface_v6: Option<u32>,
}
impl DiscoveryInterfaceBuilderAny {
    pub fn interface_v4(mut self, interface: Ipv4Addr) -> Self {
        self.interface_v4 = Some(interface);
        self
    }

    pub fn all_v4_interfaces(mut self) -> Self {
        self.interface_v4 = None;
        self
    }

    pub fn default_v4_interface(mut self) -> Self {
        self.interface_v4 = Some(Ipv4Addr::UNSPECIFIED);
        self
    }

    pub fn interface_v6(mut self, interface: NonZeroU32) -> Self {
        self.interface_v6 = Some(interface.get());
        self
    }

    pub fn all_v6_interfaces(mut self) -> Self {
        self.interface_v6 = None;
        self
    }

    pub fn default_v6_interface(mut self) -> Self {
        self.interface_v6 = Some(0);
        self
    }

    pub fn build(self) -> Result<Discovery, std::io::Error> {
        let DiscoveryBuilder {
            loopback,
            service_name,
            interval,
            responder_window: peer_window,
        } = self.builder;

        Ok(Discovery {
            socket: MdnsSocket::new(loopback, self.interface_v4, self.interface_v6)?,
            service_name,
            interval,
            peer_window,
        })
    }
}
