use crate::socket::{AsyncMdnsSocket, MdnsSocket, MdnsSocketRecv};
use std::{
    collections::BTreeSet,
    sync::{Arc, RwLock},
};
use trust_dns_client::{
    op::Message as DnsMessage,
    serialize::binary::{BinDecodable, BinEncodable, BinEncoder},
};

mod builder;
pub use builder::BroadcasterBuilder;

mod service;
use service::ServiceDnsResponse;
pub use service::{IntoServiceTxt, Service, ServiceBuilder};

mod handle;
pub use handle::BroadcasterHandle;
use handle::*;

pub(crate) struct BroadcasterConfig {
    services: BTreeSet<ServiceDnsResponse>,
}

pub struct Broadcaster {
    socket: MdnsSocket,
    config: Arc<RwLock<BroadcasterConfig>>,
}
impl Broadcaster {
    pub fn run_background(self) -> BroadcasterHandle {
        let Broadcaster { socket, config } = self;

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let config_ref = config.clone();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .thread_name("Searchlight mDNS Broadcaster (Tokio)")
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let socket = socket.into_async().await?;
                    Self::impl_run(
                        &socket,
                        socket.recv(vec![0; 4096]),
                        config_ref,
                        Some(shutdown_rx),
                    )
                    .await
                })
        });

        BroadcasterHandle(BroadcasterHandleDrop(Some(BroadcasterHandleInner {
            config,
            thread,
            shutdown_tx,
        })))
    }

    pub fn run(self) -> Result<(), std::io::Error> {
        let Broadcaster { socket, config } = self;

        tokio::runtime::Builder::new_current_thread()
            .thread_name("Searchlight mDNS Broadcaster (Tokio)")
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let socket = socket.into_async().await?;
                Self::impl_run(&socket, socket.recv(vec![0; 4096]), config, None).await
            })
    }
}
impl Broadcaster {
    async fn impl_run(
        tx: &AsyncMdnsSocket,
        mut rx: MdnsSocketRecv<'_>,
        config: Arc<RwLock<BroadcasterConfig>>,
        shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<(), std::io::Error> {
        if let Some(shutdown_rx) = shutdown_rx {
            tokio::select! {
                biased;
                res = Self::recv_loop(tx, &mut rx, &config) => res,
                _ = shutdown_rx => Ok(()),
            }
        } else {
            Self::recv_loop(tx, &mut rx, &config).await
        }
    }

    #[allow(clippy::await_holding_lock)]
    // It's fine to hold the lock in this case because we're using the current-thread runtime.
    // The future just won't be Send.
    async fn recv_loop(
        tx: &AsyncMdnsSocket,
        rx: &mut MdnsSocketRecv<'_>,
        config: &RwLock<BroadcasterConfig>,
    ) -> Result<(), std::io::Error> {
        let mut send_buf = vec![0u8; 4096];
        loop {
            let ((count, addr), packet) = rx.recv_multicast().await?;
            if count == 0 {
                continue;
            }

            let message = match DnsMessage::from_bytes(packet) {
                Ok(message) if !message.truncated() => message,
                _ => continue,
            };

            let query = match message.query() {
                Some(query) => query,
                None => continue,
            };

            for service in config
                .read()
                .unwrap()
                .services
                .iter()
                .filter(|service| service.service_type() == query.name())
            {
                send_buf.clear();

                if service
                    .dns_response
                    .emit(&mut BinEncoder::new(&mut send_buf))
                    .is_ok()
                {
                    if query.mdns_unicast_response() {
                        tx.send_to(&send_buf, addr).await?;
                    } else {
                        tx.send_multicast(&send_buf).await?;
                    }
                }
            }
        }
    }
}
