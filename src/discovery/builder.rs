use super::{errors::DiscoveryBuilderError, Discovery};
use crate::{
	errors::{BadDnsNameError, MultiIpIoError},
	net::{IpVersion, TargetInterfaceV4, TargetInterfaceV6},
	socket::MdnsSocket,
	util::IntoDnsName,
};
use std::time::Duration;
use trust_dns_client::rr::Name as DnsName;

/// A builder for [`Discovery`].
pub struct DiscoveryBuilder {
	service_name: Option<DnsName>,
	interval: Duration,
	loopback: bool,
	interface_v4: TargetInterfaceV4,
	interface_v6: TargetInterfaceV6,
	max_ignored_packets: u8,
}
impl DiscoveryBuilder {
	/// Creates a new [`DiscoveryBuilder`].
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

	/// Sets the service name to discover.
	pub fn service(mut self, service_name: impl IntoDnsName) -> Result<Self, BadDnsNameError> {
		self.service_name = Some(service_name.into_fqdn().map_err(|_| BadDnsNameError)?);
		Ok(self)
	}

	/// How often to send discovery packets.
	///
	/// I am not responsible for what happens to you if you set this too low :)
	///
	/// **Default: 10 seconds**
	pub fn interval(mut self, interval: Duration) -> Self {
		self.interval = interval;
		self
	}

	/// The number of discovery packets that a responder must ignore before it is considered to be offline.
	///
	/// If set to zero, a responder will never go offline.
	///
	/// **Default: 2**
	pub fn max_ignored_packets(mut self, max: u8) -> Self {
		self.max_ignored_packets = max;
		self
	}

	/// If loopback is enabled, any multicast packets that are sent can be received by the same socket and any other local sockets bound to the same port.
	///
	/// This is useful for testing, but is probably not very useful in production.
	pub fn loopback(mut self) -> Self {
		self.loopback = true;
		self
	}

	/// Selects the target interface for IPv4 discovery, if enabled.
	///
	/// **Default: [`TargetInterfaceV4::All`]**
	pub fn interface_v4(mut self, interface: TargetInterfaceV4) -> Self {
		self.interface_v4 = interface;
		self
	}

	/// Selects the target interface for IPv6 discovery, if enabled.
	///
	/// **Default: [`TargetInterfaceV6::All`]**
	pub fn interface_v6(mut self, interface: TargetInterfaceV6) -> Self {
		self.interface_v6 = interface;
		self
	}

	/// Builds the discoverer.
	///
	/// You must specify whether to discover over IPv4, IPv6, or both.
	pub fn build(self, ip_version: IpVersion) -> Result<Discovery, DiscoveryBuilderError> {
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
				IpVersion::V4 => {
					MdnsSocket::new_v4(loopback, interface_v4).map_err(|v4| DiscoveryBuilderError::MultiIpIoError(MultiIpIoError::V4(v4)))?
				}

				IpVersion::V6 => {
					MdnsSocket::new_v6(loopback, interface_v6).map_err(|v6| DiscoveryBuilderError::MultiIpIoError(MultiIpIoError::V6(v6)))?
				}

				IpVersion::Both => MdnsSocket::new(loopback, interface_v4, interface_v6)
					.map_err(|(v4, v6)| DiscoveryBuilderError::MultiIpIoError(MultiIpIoError::Both { v4, v6 }))?,
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
