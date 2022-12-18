use crate::socket::{AsyncMdnsSocket, MdnsSocket};
use std::{sync::Arc, time::Duration};
use trust_dns_client::{
    op::{Message as DnsMessage, Query as DnsQuery},
    rr::{DNSClass as DnsClass, Name as DnsName, RecordType as DnsRecordType},
    serialize::binary::BinEncodable,
};

mod builder;
pub use builder::*;

mod event;
pub use event::*;

mod handle;
pub use handle::*;

mod listen;

pub struct Discovery {
    socket: MdnsSocket,
    service_name: Option<DnsName>,
    interval: Duration,
    peer_window: Duration,
}
impl Discovery {
    pub fn run<F>(self, handler: F) -> DiscoveryHandle
    where
        F: Fn(DiscoveryEvent) + Send + Sync + 'static,
    {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(self.impl_run(shutdown_rx, Arc::new(handler)))
        });

        DiscoveryHandle(DiscoveryHandleDrop(Some(DiscoveryHandleInner {
            thread,
            shutdown_tx,
        })))
    }
}
impl Discovery {
    async fn impl_run(
        self,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        handler: EventHandler,
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

        tokio::select! {
            biased;
            res = tick_loop => res,
            res = listen_loop => res,
            _ = shutdown_rx => Ok(()),
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

            let packet = {
                // dns_parser version
                /*
                let mut builder = dns_parser::Builder::new_query(0, false);
                builder.add_question(
                    &service_name,
                    false,
                    dns_parser::QueryType::PTR,
                    dns_parser::QueryClass::IN,
                );
                match builder.build() {
                    Ok(packet) => packet,
                    Err(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Packet was too large",
                        ))
                    }
                }
                */

                // trust-dns version
                match DnsMessage::new()
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
                        ))
                    }
                }
            };

            socket.send_multicast(&packet).await?;
        }
    }
}
