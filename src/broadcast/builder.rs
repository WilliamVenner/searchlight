use super::{errors::BroadcasterBuilderError, service::ServiceDnsResponse, Broadcaster, BroadcasterConfig, Service};
use crate::{
	net::{IpVersion, TargetInterfaceV4, TargetInterfaceV6},
	socket::MdnsSocket,
};
use std::{
	collections::BTreeSet,
	sync::{Arc, RwLock},
};

/// Builder for [`Broadcaster`].
pub struct BroadcasterBuilder {
	services: BTreeSet<Service>,
	interface_v4: TargetInterfaceV4,
	interface_v6: TargetInterfaceV6,
	loopback: bool,
}
impl BroadcasterBuilder {
	/// Creates a new [`BroadcasterBuilder`].
	pub fn new() -> Self {
		Self {
			services: BTreeSet::new(),
			interface_v4: TargetInterfaceV4::All,
			interface_v6: TargetInterfaceV6::All,
			loopback: false,
		}
	}

	/// If loopback is enabled, any multicast packets that are sent can be received by the same socket and any other local sockets bound to the same port.
	///
	/// This is useful for testing, but is probably not very useful in production.
	pub fn loopback(mut self) -> Self {
		self.loopback = true;
		self
	}

	/// Adds a service to the broadcaster.
	///
	/// If you choose to run the broadcaster in the background (via [`Broadcaster::run_in_background`]), you can add and remove services later on.
	pub fn add_service(mut self, service: Service) -> Self {
		self.services.replace(service);
		self
	}

	/// Selects the target interface for IPv4 broadcasting, if enabled.
	///
	/// **Default: [`TargetInterfaceV4::All`]**
	pub fn interface_v4(mut self, interface: TargetInterfaceV4) -> Self {
		self.interface_v4 = interface;
		self
	}

	/// Selects the target interface for IPv6 broadcasting, if enabled.
	///
	/// **Default: [`TargetInterfaceV6::All`]**
	pub fn interface_v6(mut self, interface: TargetInterfaceV6) -> Self {
		self.interface_v6 = interface;
		self
	}

	/// Builds the broadcaster.
	///
	/// You must specify whether to broadcast over IPv4, IPv6, or both.
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
