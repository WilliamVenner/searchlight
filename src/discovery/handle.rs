use crate::errors::{MultiIpIoError, ShutdownError};

pub(super) struct DiscoveryHandleInner {
	pub(super) thread: std::thread::JoinHandle<Result<(), MultiIpIoError>>,
	pub(super) shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

pub(super) struct DiscoveryHandleDrop(pub(super) Option<DiscoveryHandleInner>);
impl DiscoveryHandleDrop {
	fn shutdown(&mut self) -> Result<(), ShutdownError> {
		let DiscoveryHandleInner { thread, shutdown_tx } = match self.0.take() {
			Some(inner) => inner,
			None => return Ok(()),
		};

		if !thread.is_finished() {
			shutdown_tx.send(()).ok();
		}

		match thread.join() {
			Ok(Ok(_)) => Ok(()),
			Ok(Err(err)) => Err(ShutdownError::MultiIpIoError(err)),
			Err(err) => Err(ShutdownError::ThreadJoinError(err)),
		}
	}
}
impl Drop for DiscoveryHandleDrop {
	fn drop(&mut self) {
		self.shutdown().ok();
	}
}

/// A handle to a [`Discovery`](super::Discovery) instance that is running in the background.
///
/// You can use this handle to shut down the discovery instance remotely.
#[must_use = "The discovery instance will shut down if the handle is dropped; store the handle somewhere or use `std::mem::forget` to keep it running"]
pub struct DiscoveryHandle(pub(super) DiscoveryHandleDrop);
impl DiscoveryHandle {
	/// Shuts down the discovery instance if it is still running.
	///
	/// This function will block until the discovery instance has shut down, and will return an error if the shutdown failed, or the discovery instance encountered a fatal error during its lifetime.
	pub fn shutdown(mut self) -> Result<(), ShutdownError> {
		let res = self.0.shutdown();
		std::mem::forget(self.0);
		res
	}
}
