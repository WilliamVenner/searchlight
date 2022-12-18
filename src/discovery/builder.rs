use super::Discovery;
use crate::socket::MdnsSocket;
use std::{net::Ipv4Addr, num::NonZeroU32, time::Duration};
use trust_dns_client::rr::{IntoName as IntoDnsName, Name as DnsName};

#[derive(Debug)]
pub struct BadNameError;
impl std::fmt::Display for BadNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Bad DNS name")
    }
}
impl std::error::Error for BadNameError {}

pub struct DiscoveryBuilder {
    service_name: Option<DnsName>,
    interval: Duration,
    peer_window: Duration,
}
impl DiscoveryBuilder {
    pub fn new() -> Self {
        Self {
            service_name: None,
            interval: Duration::from_secs(10),
            peer_window: Duration::from_secs(10),
        }
    }

    pub fn service(mut self, service_name: impl IntoDnsName) -> Result<Self, BadNameError> {
        self.service_name = Some(service_name.into_name().map_err(|_| BadNameError)?);
        Ok(self)
    }

    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub fn peer_window(mut self, peer_window: Duration) -> Self {
        self.peer_window = peer_window;
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
            service_name,
            interval,
            peer_window,
        } = self.builder;

        Ok(Discovery {
            socket: MdnsSocket::new_v4(self.interface)?,
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
            service_name,
            interval,
            peer_window,
        } = self.builder;

        Ok(Discovery {
            socket: MdnsSocket::new_v6(self.interface)?,
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
            service_name,
            interval,
            peer_window,
        } = self.builder;

        Ok(Discovery {
            socket: MdnsSocket::new(self.interface_v4, self.interface_v6)?,
            service_name,
            interval,
            peer_window,
        })
    }
}
