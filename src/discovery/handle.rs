use crate::errors::ShutdownError;

pub(super) struct DiscoveryHandleInner {
	pub(super) thread: std::thread::JoinHandle<Result<(), std::io::Error>>,
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
			Ok(Err(err)) => Err(ShutdownError::IoError(err)),
			Err(err) => Err(ShutdownError::ThreadJoinError(err)),
		}
	}
}
impl Drop for DiscoveryHandleDrop {
	fn drop(&mut self) {
		if let Err(ShutdownError::ThreadJoinError(err)) = self.shutdown() {
			Err::<(), _>(err).unwrap();
		}
	}
}

pub struct DiscoveryHandle(pub(super) DiscoveryHandleDrop);
impl DiscoveryHandle {
	pub fn shutdown(mut self) -> Result<(), ShutdownError> {
		let res = self.0.shutdown();
		std::mem::forget(self.0);
		res
	}
}
