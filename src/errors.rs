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

	#[error("I/O error occurred during Searchlight thread execution: {0}")]
	/// An I/O error occurred
	IoError(#[from] std::io::Error),
}
