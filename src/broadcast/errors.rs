use crate::errors::MultiIpIoError;

#[derive(Debug, Error)]
/// An error occurred while building a [`Service`](super::Service)
pub enum ServiceBuilderError {
	#[error("The broadcaster requires at least one advertisement address")]
	/// The broadcaster requires at least one advertisement address
	MissingAdvertisementAddr,

	#[error("TXT record too long (max 255 bytes)")]
	/// The TXT record is too long (max 255 bytes)
	RecordTooLong,
}

#[derive(Debug, Error)]
/// An error occurred while building a service DNS packet
pub enum ServiceDnsPacketBuilderError {
	#[error("There are too many IP addresses to advertise")]
	/// There are too many IP addresses to advertise
	TooManyIpAddresses,
}

#[derive(Debug, Error)]
/// An error occurred while building a [`Broadcaster`](super::Broadcaster)
pub enum BroadcasterBuilderError {
	#[error("{0}")]
	/// An error occurred while building a service DNS packet
	ServiceDnsPacketBuilderError(#[from] ServiceDnsPacketBuilderError),

	#[error("I/O error: {0}")]
	/// An I/O error occurred
	IoError(#[from] std::io::Error),

	#[error("{0}")]
	/// An I/O error occurred (on potentially both IPv4 and IPv6 sockets)
	MultiIpIoError(MultiIpIoError),
}
