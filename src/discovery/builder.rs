use super::Discovery;
use crate::{
	errors::BadDnsNameError,
	net::{IpVersion, TargetInterfaceV4, TargetInterfaceV6},
	socket::MdnsSocket,
	util::IntoDnsName,
};
use std::time::Duration;
use trust_dns_client::rr::Name as DnsName;

pub struct DiscoveryBuilder {
	service_name: Option<DnsName>,
	interval: Duration,
	loopback: bool,
	interface_v4: TargetInterfaceV4,
	interface_v6: TargetInterfaceV6,
	max_ignored_packets: u8,
}
impl DiscoveryBuilder {
	pub fn new() -> Self {
		Self {
			service_name: None,
			interval: Duration::from_secs(10),
			loopback: false,
			interface_v4: TargetInterfaceV4::All,
			interface_v6: TargetInterfaceV6::All,
			max_ignored_packets: 2,
		}
	}

	pub fn service(mut self, service_name: impl IntoDnsName) -> Result<Self, BadDnsNameError> {
		self.service_name = Some(service_name.into_fqdn().map_err(|_| BadDnsNameError)?);
		Ok(self)
	}

	pub fn interval(mut self, interval: Duration) -> Self {
		self.interval = interval;
		self
	}

	/// The number of discovery packets that a responder must ignore before it is considered to be offline.
	///
	/// If set to zero, a responder will never go offline.
	pub fn max_ignored_packets(mut self, max: u8) -> Self {
		self.max_ignored_packets = max;
		self
	}

	pub fn loopback(mut self) -> Self {
		self.loopback = true;
		self
	}

	pub fn interface_v4(mut self, interface: TargetInterfaceV4) -> Self {
		self.interface_v4 = interface;
		self
	}

	pub fn interface_v6(mut self, interface: TargetInterfaceV6) -> Self {
		self.interface_v6 = interface;
		self
	}

	pub fn build(self, ip_version: IpVersion) -> Result<Discovery, std::io::Error> {
		let DiscoveryBuilder {
			service_name,
			interval,
			loopback,
			interface_v4,
			interface_v6,
			max_ignored_packets,
		} = self;

		Ok(Discovery {
			socket: match ip_version {
				IpVersion::V4 => MdnsSocket::new_v4(loopback, interface_v4)?,
				IpVersion::V6 => MdnsSocket::new_v6(loopback, interface_v6)?,
				IpVersion::Both => MdnsSocket::new(loopback, interface_v4, interface_v6)?,
			},

			max_ignored_packets,
			service_name,
			interval,
		})
	}
}
impl Default for DiscoveryBuilder {
	fn default() -> Self {
		Self::new()
	}
}
