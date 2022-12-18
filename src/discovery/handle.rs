pub(super) struct DiscoveryHandleInner {
    pub(super) thread: std::thread::JoinHandle<Result<(), std::io::Error>>,
    pub(super) shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

pub(super) struct DiscoveryHandleDrop(pub(super) Option<DiscoveryHandleInner>);
impl DiscoveryHandleDrop {
    fn shutdown(&mut self) -> std::thread::Result<Result<(), std::io::Error>> {
        let DiscoveryHandleInner {
            thread,
            shutdown_tx,
        } = match self.0.take() {
            Some(inner) => inner,
            None => unreachable!(),
        };

        shutdown_tx.send(()).ok();
        thread.join()
    }
}
impl Drop for DiscoveryHandleDrop {
    fn drop(&mut self) {
        self.shutdown().unwrap().unwrap();
    }
}

pub struct DiscoveryHandle(pub(super) DiscoveryHandleDrop);
impl DiscoveryHandle {
    pub fn shutdown(mut self) -> std::thread::Result<Result<(), std::io::Error>> {
        self.0.shutdown()
    }
}
