use crate::errors::MultiIpIoError;

#[derive(Debug, Error)]
/// An error occurred while building a [`Discovery`](super::Discovery)
pub enum DiscoveryBuilderError {
	#[error("{0}")]
	/// An I/O error occurred (on potentially both IPv4 and IPv6 sockets)
	MultiIpIoError(MultiIpIoError),
}
