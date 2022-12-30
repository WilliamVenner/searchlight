use std::any::Any;

#[derive(Debug)]
pub struct BadDnsNameError;
impl std::fmt::Display for BadDnsNameError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("Bad DNS name")
	}
}
impl std::error::Error for BadDnsNameError {}

#[derive(Debug, Error)]
pub enum ShutdownError {
	#[error("Thread panicked")]
	ThreadJoinError(Box<dyn Any + Send + 'static>),

	#[error("I/O error occurred during Searchlight thread execution: {0}")]
	IoError(#[from] std::io::Error),
}
