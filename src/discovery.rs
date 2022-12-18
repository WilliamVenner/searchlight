use crate::socket::{AsyncMdnsSocket, MdnsSocket};
use std::{sync::Arc, time::Duration};
use trust_dns_client::{
    op::{Message as DnsMessage, Query as DnsQuery},
    rr::{DNSClass as DnsClass, Name as DnsName, RecordType as DnsRecordType},
    serialize::binary::BinEncodable,
};

mod builder;
pub use builder::DiscoveryBuilder;

mod event;
pub use event::DiscoveryEvent;
use event::*;

mod handle;
pub use handle::DiscoveryHandle;
use handle::*;

mod listen;
pub use listen::Responder;

pub struct Discovery {
    socket: MdnsSocket,
    service_name: Option<DnsName>,
    interval: Duration,
    peer_window: Duration,
}
impl Discovery {
    pub fn run_in_background<F>(self, handler: F) -> DiscoveryHandle
    where
        F: Fn(DiscoveryEvent) + Send + Sync + 'static,
    {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .thread_name("Searchlight mDNS Discovery (Tokio)")
                .enable_all()
                .build()
                .unwrap()
                .block_on(self.impl_run(Arc::new(handler), Some(shutdown_rx)))
        });

        DiscoveryHandle(DiscoveryHandleDrop(Some(DiscoveryHandleInner {
            thread,
            shutdown_tx,
        })))
    }

    pub fn run<F>(self, handler: F) -> Result<(), std::io::Error>
    where
        F: Fn(DiscoveryEvent) + Send + Sync + 'static,
    {
        tokio::runtime::Builder::new_current_thread()
            .thread_name("Searchlight mDNS Discovery (Tokio)")
            .enable_all()
            .build()
            .unwrap()
            .block_on(self.impl_run(Arc::new(handler), None))
    }
}
impl Discovery {
    async fn impl_run(
        self,
        handler: EventHandler,
        shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<(), std::io::Error> {
        let Discovery {
            socket,
            service_name,
            interval,
            peer_window,
        } = self;

        let socket = socket.into_async().await?;

        let tick_loop = Self::tick_loop(service_name.clone(), interval, &socket);
        let listen_loop = Self::listen_loop(service_name, handler, peer_window, &socket);

        let shutdown = async move {
            if let Some(shutdown_rx) = shutdown_rx {
                shutdown_rx.await
            } else {
                std::future::pending().await
            }
        };

        tokio::select! {
            biased;
            res = tick_loop => res,
            res = listen_loop => res,
            _ = shutdown => Ok(()),
        }
    }

    async fn tick_loop(
        service_name: Option<DnsName>,
        interval: Duration,
        socket: &AsyncMdnsSocket,
    ) -> Result<(), std::io::Error> {
        let mut interval = tokio::time::interval(interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let packet = match DnsMessage::new()
                .add_query({
                    let mut query = DnsQuery::new();

                    if let Some(service_name) = &service_name {
                        query.set_name(service_name.clone());
                    }

                    query
                        .set_query_type(DnsRecordType::PTR)
                        .set_query_class(DnsClass::IN)
                        .set_mdns_unicast_response(false);

                    query
                })
                .to_bytes()
            {
                Ok(packet) => packet,
                Err(err) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Packet was too large: {err}"),
                    ));
                }
            };

            socket.send_multicast(&packet).await?;
        }
    }
}
