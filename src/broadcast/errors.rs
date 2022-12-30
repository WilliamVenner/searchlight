#[derive(Debug, Error)]
pub enum ServiceBuilderError {
	#[error("The broadcaster requires at least one advertisement address")]
	MissingAdvertisementAddr,
	#[error("TXT record too long (max 255 bytes)")]
	RecordTooLong,
}

#[derive(Debug, Error)]
pub enum ServiceDnsPacketBuilderError {
	#[error("There are too many IP addresses to advertise")]
	TooManyIpAddresses,
}

#[derive(Debug, Error)]
pub enum BroadcasterBuilderError {
	#[error("{0}")]
	ServiceDnsPacketBuilderError(#[from] ServiceDnsPacketBuilderError),

	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),
}
