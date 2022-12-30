use super::errors::{ServiceBuilderError, ServiceDnsPacketBuilderError};
use crate::{errors::BadDnsNameError, util::IntoDnsName};
use std::{
	borrow::{Borrow, Cow},
	collections::BTreeSet,
	net::IpAddr,
	ops::Deref,
};
use trust_dns_client::{
	op::{Header as DnsHeader, Message as DnsMessage, MessageType as DnsMessageType, OpCode as DnsOpCode},
	rr::{
		rdata::{SRV, TXT},
		DNSClass as DnsClass, Name as DnsName, RData, Record as DnsRecord, RecordType as DnsRecordType,
	},
};

const TXT_MAX_LEN: usize = 255;

/// Convenience trait implemented for types that can be interpreted as a DNS TXT record.
///
/// Searchlight allows you to configure a service with TXT records that may or may not be `'static`, so this trait allows you to enter both static and runtime values as TXT records.
pub trait IntoServiceTxt: Sized {
	/// Converts this value into a [`Cow`] of a byte slice, which can be used as a TXT record.
	fn into_service_txt(self) -> Cow<'static, [u8]>;

	/// Same as [`into_service_txt`](IntoServiceTxt::into_service_txt), but truncates the value to 255 bytes.
	fn into_service_txt_truncated(self) -> Cow<'static, [u8]>;
}
impl IntoServiceTxt for Vec<u8> {
	#[inline(always)]
	fn into_service_txt(self) -> Cow<'static, [u8]> {
		Cow::Owned(self)
	}

	#[inline(always)]
	fn into_service_txt_truncated(mut self) -> Cow<'static, [u8]> {
		self.truncate(TXT_MAX_LEN);
		self.into_service_txt()
	}
}
impl IntoServiceTxt for &'static [u8] {
	#[inline(always)]
	fn into_service_txt(self) -> Cow<'static, [u8]> {
		Cow::Borrowed(self)
	}

	#[inline(always)]
	fn into_service_txt_truncated(self) -> Cow<'static, [u8]> {
		Cow::Borrowed(&self[..TXT_MAX_LEN.min(self.len())])
	}
}
impl IntoServiceTxt for String {
	#[inline(always)]
	fn into_service_txt(self) -> Cow<'static, [u8]> {
		Cow::Owned(self.into_bytes())
	}

	#[inline(always)]
	fn into_service_txt_truncated(self) -> Cow<'static, [u8]> {
		self.into_bytes().into_service_txt_truncated()
	}
}
impl IntoServiceTxt for &'static str {
	#[inline(always)]
	fn into_service_txt(self) -> Cow<'static, [u8]> {
		Cow::Borrowed(self.as_bytes())
	}

	#[inline(always)]
	fn into_service_txt_truncated(self) -> Cow<'static, [u8]> {
		self.as_bytes().into_service_txt_truncated()
	}
}
impl<const N: usize> IntoServiceTxt for &'static [u8; N] {
	#[inline(always)]
	fn into_service_txt(self) -> Cow<'static, [u8]> {
		Cow::Borrowed(self)
	}

	#[inline(always)]
	fn into_service_txt_truncated(self) -> Cow<'static, [u8]> {
		if N > TXT_MAX_LEN {
			Cow::Borrowed(&self[..TXT_MAX_LEN])
		} else {
			self.into_service_txt()
		}
	}
}

#[derive(Debug)]
pub struct ServiceDnsResponse {
	service: Service,
	pub dns_response: DnsMessage,
}
impl TryFrom<Service> for ServiceDnsResponse {
	type Error = ServiceDnsPacketBuilderError;

	fn try_from(service: Service) -> Result<Self, Self::Error> {
		service.dns_response().map(|dns_response| Self { service, dns_response })
	}
}
impl Deref for ServiceDnsResponse {
	type Target = Service;

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		&self.service
	}
}
impl Borrow<Service> for ServiceDnsResponse {
	#[inline(always)]
	fn borrow(&self) -> &Service {
		&self.service
	}
}
impl PartialOrd for ServiceDnsResponse {
	#[inline(always)]
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		self.service.partial_cmp(&other.service)
	}
}
impl Ord for ServiceDnsResponse {
	#[inline(always)]
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.service.cmp(&other.service)
	}
}
impl PartialEq for ServiceDnsResponse {
	#[inline(always)]
	fn eq(&self, other: &Self) -> bool {
		self.service.eq(&other.service)
	}
}
impl Eq for ServiceDnsResponse {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A service that can be advertised on the network.
///
/// This can be created using the [`ServiceBuilder`].
pub struct Service {
	service_type: DnsName,
	service_name: DnsName,
	service_hostname: DnsName,
	service_id: DnsName,
	pub(crate) service_subtype_suffix: Option<String>,
	ip_addresses: BTreeSet<IpAddr>,
	port: u16,
	txt: Vec<Cow<'static, [u8]>>,
	ttl: u32,
}
impl Service {
	#[inline(always)]
	/// The service type.
	pub fn service_type(&self) -> &DnsName {
		&self.service_type
	}

	#[inline(always)]
	/// The service name.
	pub fn service_name(&self) -> &DnsName {
		&self.service_name
	}

	#[inline(always)]
	/// The IP addresses this service is available on.
	pub fn ip_addresses(&self) -> &BTreeSet<IpAddr> {
		&self.ip_addresses
	}

	#[inline(always)]
	/// The port this service is available on.
	pub fn port(&self) -> u16 {
		self.port
	}

	#[inline(always)]
	/// The TTL of this service record when it is advertised over DNS.
	pub fn ttl(&self) -> u32 {
		self.ttl
	}

	#[inline(always)]
	/// The TXT records of this service.
	pub fn txt(&self) -> &Vec<Cow<'static, [u8]>> {
		&self.txt
	}

	#[inline(always)]
	/// Whether the service can be subtyped.
	pub fn can_subtype(&self) -> bool {
		self.service_subtype_suffix.is_some()
	}

	/// Builds a DNS packet that can be sent to a client to respond to a DNS query for this service.
	pub fn dns_response(&self) -> Result<DnsMessage, ServiceDnsPacketBuilderError> {
		let mut response = DnsMessage::new();

		response.set_header({
			let mut header = DnsHeader::new();
			header.set_authoritative(true);
			header.set_message_type(DnsMessageType::Response);
			header.set_op_code(DnsOpCode::Query);
			header.set_answer_count(1);
			header.set_additional_count(
				(self.ip_addresses.len() + 1 + 1)
					.try_into()
					.map_err(|_| ServiceDnsPacketBuilderError::TooManyIpAddresses)?,
			);
			header
		});

		response.add_answer({
			let mut record = DnsRecord::new();

			record
				.set_dns_class(DnsClass::IN)
				.set_rr_type(DnsRecordType::PTR)
				.set_data(Some(RData::PTR(self.service_id.clone())))
				.set_name(self.service_type.clone())
				.set_ttl(self.ttl);

			record
		});

		for addr in self.ip_addresses.iter() {
			response.add_additional({
				let mut record = DnsRecord::new();

				record
					.set_dns_class(DnsClass::IN)
					.set_rr_type(match addr {
						IpAddr::V4(_) => DnsRecordType::A,
						IpAddr::V6(_) => DnsRecordType::AAAA,
					})
					.set_data(Some(match addr {
						IpAddr::V4(addr) => RData::A(*addr),
						IpAddr::V6(addr) => RData::AAAA(*addr),
					}))
					.set_name(self.service_hostname.clone())
					.set_ttl(self.ttl)
					.set_mdns_cache_flush(true);

				record
			});
		}

		response.add_additional({
			let mut record = DnsRecord::new();

			record
				.set_dns_class(DnsClass::IN)
				.set_rr_type(DnsRecordType::SRV)
				.set_data(Some(RData::SRV(SRV::new(0, 0, self.port, self.service_hostname.clone()))))
				.set_name(self.service_id.clone())
				.set_ttl(self.ttl);

			record
		});

		response.add_additional({
			let mut record = DnsRecord::new();

			record
				.set_dns_class(DnsClass::IN)
				.set_rr_type(DnsRecordType::TXT)
				.set_data(Some(RData::TXT(TXT::from_bytes(
					self.txt.iter().map(|txt| txt.as_ref()).collect::<Vec<&[u8]>>(),
				))))
				.set_name(self.service_id.clone())
				.set_ttl(self.ttl)
				.set_mdns_cache_flush(true);

			record
		});

		Ok(response)
	}
}

/// A builder for [`Service`]s.
pub struct ServiceBuilder(Service);
impl ServiceBuilder {
	/// Creates a new [`ServiceBuilder`] for a service with the given type, name, and port.
	pub fn new(service_type: impl IntoDnsName, service_name: impl IntoDnsName, port: u16) -> Result<Self, BadDnsNameError> {
		let service_type = service_type.into_fqdn().map_err(|_| BadDnsNameError)?;
		let service_name = service_name.into_fqdn().map_err(|_| BadDnsNameError)?;
		Ok(Self(Service {
			service_id: format!("{service_name}{service_type}").into_fqdn().map_err(|_| BadDnsNameError)?,

			service_hostname: format!("{service_name}local.").into_fqdn().map_err(|_| BadDnsNameError)?,

			service_type,
			service_name,
			service_subtype_suffix: None,
			ip_addresses: BTreeSet::new(),
			port,
			txt: Vec::new(),
			ttl: 120,
		}))
	}

	/// Sets the TTL of the service.
	pub fn ttl(mut self, ttl: u32) -> Self {
		self.0.ttl = ttl;
		self
	}

	#[inline(always)]
	/// Adds an IP address that the service is available on.
	pub fn add_ip_address(mut self, ip_address: IpAddr) -> Self {
		self.0.ip_addresses.insert(ip_address);
		self
	}

	#[inline(always)]
	/// Adds a TXT record to the service.
	pub fn add_txt(mut self, record: impl IntoServiceTxt) -> Self {
		self.0.txt.push(record.into_service_txt());
		self
	}

	#[inline(always)]
	/// Adds a TXT record to the service, truncating it if it is too long (more than 255 bytes)
	pub fn add_txt_truncated(mut self, record: impl IntoServiceTxt) -> Self {
		self.0.txt.push(record.into_service_txt());
		self
	}

	#[inline(always)]
	/// Can this service be subtyped? If so, the broadcaster will respond to queries with subtyped service types.
	pub fn can_subtype(mut self) -> Result<Self, BadDnsNameError> {
		self.0.service_subtype_suffix = Some(format!(
			".{}",
			format!("_sub.{}", self.0.service_type)
				.into_fqdn()
				.map_err(|_| BadDnsNameError)?
				.to_utf8()
		));
		Ok(self)
	}

	/// Builds the [`Service`].
	pub fn build(self) -> Result<Service, ServiceBuilderError> {
		if self.0.ip_addresses.is_empty() {
			return Err(ServiceBuilderError::MissingAdvertisementAddr);
		}

		if !self.0.txt.iter().all(|txt| txt.len() <= TXT_MAX_LEN) {
			return Err(ServiceBuilderError::RecordTooLong);
		}

		Ok(self.0)
	}
}
