use super::BroadcasterConfig;
use crate::{
    errors::{BadNameError, ServiceDnsPacketError},
    util::IntoDnsName,
    Service,
};
use std::sync::{Arc, RwLock};

pub(super) struct BroadcasterHandleInner {
    pub(super) config: Arc<RwLock<BroadcasterConfig>>,
    pub(super) thread: std::thread::JoinHandle<Result<(), std::io::Error>>,
    pub(super) shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

pub(super) struct BroadcasterHandleDrop(pub(super) Option<BroadcasterHandleInner>);
impl BroadcasterHandleDrop {
    fn shutdown(&mut self) -> std::thread::Result<Result<(), std::io::Error>> {
        let BroadcasterHandleInner {
            thread,
            shutdown_tx,
            ..
        } = match self.0.take() {
            Some(inner) => inner,
            None => return Ok(Ok(())),
        };

        if thread.is_finished() {
            return Ok(Ok(()));
        }

        shutdown_tx.send(()).ok();
        thread.join()
    }
}
impl Drop for BroadcasterHandleDrop {
    fn drop(&mut self) {
        self.shutdown().unwrap().unwrap();
    }
}

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

    pub fn shutdown(mut self) -> std::thread::Result<Result<(), std::io::Error>> {
        let res = self.0.shutdown();
        std::mem::forget(self.0);
        res
    }

    pub fn add_service(&self, service: Service) -> Result<(), ServiceDnsPacketError> {
        match self.with_config(|broadcaster| {
            Ok(broadcaster
                .write()
                .unwrap()
                .services
                .insert(service.try_into()?))
        }) {
            Some(Ok(_)) | None => Ok(()),
            Some(Err(err)) => Err(err),
        }
    }

    pub fn remove_named_service(
        &self,
        service_type: impl IntoDnsName,
        service_name: impl IntoDnsName,
    ) -> Result<(), BadNameError> {
        let service_type = service_type.into_fqdn().map_err(|_| BadNameError)?;
        let service_name = service_name.into_fqdn().map_err(|_| BadNameError)?;
        self.with_config(|broadcaster| {
            broadcaster.write().unwrap().services.retain(|service| {
                *service.service_name() != service_name || *service.service_type() != service_type
            })
        });
        Ok(())
    }

    pub fn remove_service(&self, service: &Service) {
        self.with_config(|broadcaster| broadcaster.write().unwrap().services.remove(service));
    }
}
