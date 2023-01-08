//! Errors that can occur when using this crate

use std::any::Any;

#[derive(Debug)]
/// A DNS name is invalid
pub struct BadDnsNameError;
impl std::fmt::Display for BadDnsNameError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("Bad DNS name")
	}
}
impl std::error::Error for BadDnsNameError {}

#[derive(Debug, Error)]
/// An error occurred while shutting down a broadcaster or discoverer
pub enum ShutdownError {
	#[error("Thread panicked")]
	/// The underlying thread panicked
	ThreadJoinError(Box<dyn Any + Send + 'static>),

	#[error("During Searchlight thread execution: {0}")]
	/// An I/O error occurred
	MultiIpIoError(#[from] MultiIpIoError),
}

#[derive(Debug, Error)]
/// Because this crate works with both IPv4 and IPv6 sockets under a single interface, it is possible for an I/O error to occur on both sockets. This enum is used to represent that.
///
/// For convenience, this enum also has a generic [`IoError`](`MultiIpIoError::IoError`) variant that can be used to represent an I/O error that is not specific to any socket (not to be confused with [`Both`](MultiIpIoError::Both))
pub enum MultiIpIoError {
	#[error("I/O error: {0}")]
	/// A generic I/O error occurred from something other than a socket.
	IoError(#[from] std::io::Error),

	#[error("I/O error: {0} (IPv4)")]
	/// An I/O error occurred on the IPv4 socket
	V4(std::io::Error),

	#[error("I/O error: {0} (IPv6)")]
	/// An I/O error occurred on the IPv6 socket
	V6(std::io::Error),

	#[error("I/O error: {v4} (IPv4) {v6} (IPv6)")]
	/// An I/O error occurred on both IPv4 and IPv6 sockets
	Both {
		/// The IPv4 I/O error
		v4: std::io::Error,

		/// The IPv6 I/O error
		v6: std::io::Error,
	},
}
