use super::Discovery;
use crate::{
	errors::BadNameError,
	socket::{IpVersion, MdnsSocket, TargetInterface},
	util::IntoDnsName,
};
use std::{net::Ipv4Addr, num::NonZeroU32, time::Duration};
use trust_dns_client::rr::Name as DnsName;

pub struct DiscoveryBuilder {
	service_name: Option<DnsName>,
	interval: Duration,
	responder_window: Duration,
	loopback: bool,
	interface_v4: TargetInterface<Ipv4Addr>,
	interface_v6: TargetInterface<NonZeroU32>,
}
impl DiscoveryBuilder {
	pub fn new() -> Self {
		Self {
			service_name: None,
			interval: Duration::from_secs(10),
			responder_window: Duration::from_secs(10),
			loopback: false,
			interface_v4: TargetInterface::All,
			interface_v6: TargetInterface::All,
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
	/// If a responder goes quiet for this amount of time, it is considered "lost" and will emit a [`DiscoveryEvent::ResponderLost`](crate::discovery::DiscoveryEvent::ResponderLost) event.
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

	pub fn interface_v4(mut self, interface: TargetInterface<Ipv4Addr>) -> Self {
		self.interface_v4 = interface;
		self
	}

	pub fn interface_v6(mut self, interface: TargetInterface<NonZeroU32>) -> Self {
		self.interface_v6 = interface;
		self
	}

	pub fn build(self, ip_version: IpVersion) -> Result<Discovery, std::io::Error> {
		let DiscoveryBuilder {
			service_name,
			interval,
			responder_window: peer_window,
			loopback,
			interface_v4,
			interface_v6,
		} = self;

		Ok(Discovery {
			socket: match ip_version {
				IpVersion::V4 => MdnsSocket::new_v4(loopback, interface_v4)?,
				IpVersion::V6 => MdnsSocket::new_v6(loopback, interface_v6)?,
				IpVersion::Both => MdnsSocket::new(loopback, interface_v4, interface_v6)?,
			},

			service_name,
			interval,
			peer_window,
		})
	}
}
impl Default for DiscoveryBuilder {
	fn default() -> Self {
		Self::new()
	}
}
