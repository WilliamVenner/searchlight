use super::{service::ServiceDnsResponse, Broadcaster, BroadcasterConfig, Service};
use crate::{
	errors::BroadcasterBuilderError,
	socket::{IpVersion, MdnsSocket, TargetInterface},
};
use std::{
	collections::BTreeSet,
	net::Ipv4Addr,
	num::NonZeroU32,
	sync::{Arc, RwLock},
};

pub struct BroadcasterBuilder {
	services: BTreeSet<Service>,
	interface_v4: TargetInterface<Ipv4Addr>,
	interface_v6: TargetInterface<NonZeroU32>,
	loopback: bool,
}
impl BroadcasterBuilder {
	pub fn new() -> Self {
		Self {
			services: BTreeSet::new(),
			interface_v4: TargetInterface::All,
			interface_v6: TargetInterface::All,
			loopback: false,
		}
	}

	pub fn loopback(mut self) -> Self {
		self.loopback = true;
		self
	}

	pub fn add_service(mut self, service: Service) -> Self {
		self.services.replace(service);
		self
	}

	pub fn interface_v4(mut self, interface: TargetInterface<Ipv4Addr>) -> Self {
		self.interface_v4 = interface;
		self
	}

	pub fn interface_v6(mut self, interface: TargetInterface<NonZeroU32>) -> Self {
		self.interface_v6 = interface;
		self
	}

	pub fn build(self, ip_version: IpVersion) -> Result<Broadcaster, BroadcasterBuilderError> {
		let BroadcasterBuilder {
			services,
			interface_v4,
			interface_v6,
			loopback,
		} = self;

		Ok(Broadcaster {
			socket: match ip_version {
				IpVersion::V4 => MdnsSocket::new_v4(loopback, interface_v4)?,
				IpVersion::V6 => MdnsSocket::new_v6(loopback, interface_v6)?,
				IpVersion::Both => MdnsSocket::new(loopback, interface_v4, interface_v6)?,
			},

			config: Arc::new(RwLock::new(BroadcasterConfig {
				services: {
					let mut dns_services = BTreeSet::new();
					for service in services {
						dns_services.replace(ServiceDnsResponse::try_from(service)?);
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
