use super::{errors::ServiceDnsPacketBuilderError, BroadcasterConfig, Service};
use crate::{
	errors::{BadDnsNameError, MultiIpIoError, ShutdownError},
	util::IntoDnsName,
};
use std::sync::{Arc, RwLock};

pub(super) struct BroadcasterHandleInner {
	pub(super) config: Arc<RwLock<BroadcasterConfig>>,
	pub(super) thread: std::thread::JoinHandle<Result<(), MultiIpIoError>>,
	pub(super) shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

pub(super) struct BroadcasterHandleDrop(pub(super) Option<BroadcasterHandleInner>);
impl BroadcasterHandleDrop {
	fn shutdown(&mut self) -> Result<(), ShutdownError> {
		let BroadcasterHandleInner { thread, shutdown_tx, .. } = match self.0.take() {
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
impl Drop for BroadcasterHandleDrop {
	fn drop(&mut self) {
		if let Err(ShutdownError::ThreadJoinError(err)) = self.shutdown() {
			Err::<(), _>(err).unwrap();
		}
	}
}

/// A handle to a [`Broadcaster`](super::Broadcaster) instance that is running in the background.
///
/// You can use this handle to shut down the broadcaster instance remotely, and to add or remove services.
#[must_use = "The broadcaster instance will shut down if the handle is dropped; store the handle somewhere or use `std::mem::forget` to keep it running"]
pub struct BroadcasterHandle(pub(super) BroadcasterHandleDrop);
impl BroadcasterHandle {
	#[inline(always)]
	fn with_config<F, R>(&self, handle: F) -> Option<R>
	where
		F: FnOnce(&RwLock<BroadcasterConfig>) -> R,
	{
		let config = match &self.0 .0.as_ref() {
			Some(inner) => &inner.config,
			None => return None,
		};

		Some(handle(config))
	}

	/// Shuts down the broadcaster instance if it is still running.
	///
	/// This function will block until the broadcaster instance has shut down, and will return an error if the shutdown failed, or the broadcaster instance encountered a fatal error during its lifetime.
	pub fn shutdown(mut self) -> Result<(), ShutdownError> {
		let res = self.0.shutdown();
		std::mem::forget(self.0);
		res
	}

	/// Adds a service to the broadcaster configuration.
	///
	/// The service will be broadcasted at the next opportunity.
	pub fn add_service(&self, service: Service) -> Result<(), ServiceDnsPacketBuilderError> {
		match self.with_config(|broadcaster| Ok(broadcaster.write().unwrap().services.replace(service.try_into()?))) {
			Some(Ok(_)) | None => Ok(()),
			Some(Err(err)) => Err(err),
		}
	}

	/// Removes a service from the broadcaster configuration, finding it by name.
	///
	/// Returns `true` if the service was found and removed, and `false` if it was not found.
	pub fn remove_named_service(&self, service_type: impl IntoDnsName, service_name: impl IntoDnsName) -> Result<bool, BadDnsNameError> {
		let service_type = service_type.into_fqdn().map_err(|_| BadDnsNameError)?;
		let service_name = service_name.into_fqdn().map_err(|_| BadDnsNameError)?;

		let mut found = false;
		self.with_config(|broadcaster| {
			broadcaster.write().unwrap().services.retain(|service| {
				if *service.service_name() != service_name || *service.service_type() != service_type {
					true
				} else {
					found = true;
					false
				}
			})
		});

		Ok(found)
	}

	/// Removes a service from the broadcaster configuration, finding it by type.
	///
	/// Returns `true` if the service was found and removed, and `false` if it was not found.
	pub fn remove_service_type(&self, service_type: impl IntoDnsName) -> Result<bool, BadDnsNameError> {
		let service_type = service_type.into_fqdn().map_err(|_| BadDnsNameError)?;

		let mut found = false;
		self.with_config(|broadcaster| {
			broadcaster.write().unwrap().services.retain(|service| {
				if *service.service_type() != service_type {
					true
				} else {
					found = true;
					false
				}
			})
		});

		Ok(found)
	}

	/// Removes a service from the broadcaster configuration, finding it by name and type via an existing [`Service`] reference.
	///
	/// Returns `true` if the service was found and removed, and `false` if it was not found.
	pub fn remove_service(&self, service: &Service) {
		self.with_config(|broadcaster| broadcaster.write().unwrap().services.remove(service));
	}
}
