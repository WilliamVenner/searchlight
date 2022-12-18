use super::{service::ServiceDnsResponse, BroadcasterConfig};
use crate::{errors::BroadcasterBuilderError, socket::MdnsSocket, Broadcaster, Service};
use std::{
    collections::BTreeSet,
    net::Ipv4Addr,
    num::NonZeroU32,
    sync::{Arc, RwLock},
};

pub struct BroadcasterBuilder {
    services: BTreeSet<Service>,
    interface_v4: Option<Ipv4Addr>,
    interface_v6: Option<u32>,
    loopback: bool,
}
impl BroadcasterBuilder {
    pub fn new() -> Self {
        Self {
            services: BTreeSet::new(),
            interface_v4: None,
            interface_v6: None,
            loopback: false,
        }
    }

    pub fn loopback(mut self) -> Self {
        self.loopback = true;
        self
    }

    pub fn add_service(mut self, service: Service) -> Self {
        self.services.insert(service);
        self
    }

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

    pub fn build(self) -> Result<Broadcaster, BroadcasterBuilderError> {
        // TODO allow user to choose ipv4 only or ipv6 only
        let socket = MdnsSocket::new(self.loopback, self.interface_v4, self.interface_v6)?;

        Ok(Broadcaster {
            socket,
            config: Arc::new(RwLock::new(BroadcasterConfig {
                services: {
                    let mut dns_services = BTreeSet::new();
                    for service in self.services {
                        dns_services.insert(ServiceDnsResponse::try_from(service)?);
                    }
                    dns_services
                },
            })),
        })
    }
}
impl Default for BroadcasterBuilder {
    fn default() -> Self {
        Self::new()
    }
}
