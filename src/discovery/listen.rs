use super::EventHandler;
use crate::{socket::AsyncMdnsSocket, DiscoveryEvent};
use std::{
    borrow::Borrow,
    collections::HashSet,
    hash::Hash,
    net::SocketAddr,
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};
use trust_dns_client::{
    op::{DnsResponse, Message},
    rr::Name as DnsName,
    serialize::binary::BinDecodable,
};

#[derive(Debug, Clone)]
pub struct Responder {
    pub addr: SocketAddr,
    pub last_response: DnsResponse,
    pub last_responded: Instant,
}

#[derive(Clone)]
struct ResponderMemoryEntry(Arc<Responder>);
impl Deref for ResponderMemoryEntry {
    type Target = Responder;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Borrow<SocketAddr> for ResponderMemoryEntry {
    fn borrow(&self) -> &SocketAddr {
        &self.addr
    }
}
impl Hash for ResponderMemoryEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.addr.hash(state);
    }
}
impl PartialEq for ResponderMemoryEntry {
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr
    }
}
impl Eq for ResponderMemoryEntry {}

struct ResponderMemory {
    window_interval: tokio::time::Interval,
    event_handler: EventHandler,
    memory: HashSet<ResponderMemoryEntry>,
    window: Duration,
    post_sweep_tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl ResponderMemory {
    fn get(&self, addr: &SocketAddr) -> Option<&ResponderMemoryEntry> {
        self.memory.get(addr)
    }

    fn insert(&mut self, entry: ResponderMemoryEntry) {
        self.memory.insert(entry);
    }

    async fn sweep(&mut self) {
        self.post_sweep_tasks.clear();

        self.memory.retain(|entry| {
            if entry.last_responded.elapsed() < self.window {
                return true;
            }

            let responder = entry.0.clone();
            let event_handler = self.event_handler.clone();
            self.post_sweep_tasks
                .push(tokio::task::spawn_blocking(move || {
                    event_handler(DiscoveryEvent::ResponderLost(responder))
                }));

            false
        });

        for task in self.post_sweep_tasks.drain(..) {
            task.await.ok();
        }
    }
}

impl super::Discovery {
    async fn recv_multicast(
        service_name: Option<&DnsName>,
        event_handler: &EventHandler,
        response_memory_bank: &mut ResponderMemory,
        recv: ((usize, SocketAddr), &[u8]),
    ) {
        let ((count, addr), packet) = recv;

        if count == 0 {
            return;
        }

        let response = match Message::from_bytes(&packet[..count]) {
            Ok(response) => DnsResponse::from(response),
            Err(_) => return,
        };

        if let Some(service_name) = service_name {
            if !response
                .answers()
                .iter()
                .any(|answer| answer.name() == service_name)
            {
                // This response does not contain the service we are looking for.
                return;
            }
        }

        let event = {
            let old = response_memory_bank
                .get(&addr)
                .map(|response_memory| response_memory.0.clone());

            let new = {
                let responder = Arc::new(Responder {
                    addr,
                    last_response: response,
                    last_responded: Instant::now(),
                });
                response_memory_bank.insert(ResponderMemoryEntry(responder.clone()));
                responder
            };

            match old {
                Some(old) => DiscoveryEvent::ResponseUpdate { old, new },
                None => DiscoveryEvent::ResponderFound(new),
            }
        };

        let event_handler = event_handler.clone();
        tokio::task::spawn_blocking(move || event_handler(event))
            .await
            .ok();
    }

    pub(super) async fn listen_loop(
        service_name: Option<DnsName>,
        event_handler: EventHandler,
        peer_window: Duration,
        socket: &AsyncMdnsSocket,
    ) -> Result<(), std::io::Error> {
        let mut socket = socket.recv(vec![0; 4096]);

        let mut peers_memory = ResponderMemory {
            memory: HashSet::new(),
            post_sweep_tasks: Vec::new(),

            window: peer_window,
            event_handler: event_handler.clone(),

            window_interval: {
                let mut window_interval = tokio::time::interval(peer_window.div_f32(2.0));
                window_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                window_interval.tick().await;
                window_interval
            },
        };

        let service_name = service_name.as_ref();
        loop {
            tokio::select! {
                recv = socket.recv_multicast() => {
                    let recv = recv?;
                    Self::recv_multicast(service_name, &event_handler, &mut peers_memory, recv).await
                },

                _ = peers_memory.window_interval.tick() => {
                    peers_memory.sweep().await
                },
            }
        }
    }
}
