use super::{event::EventHandler, DiscoveryEvent};
use std::{borrow::Borrow, cell::Cell, collections::HashSet, hash::Hash, net::SocketAddr, ops::Deref, sync::Arc, time::Instant};
use trust_dns_client::op::DnsResponse;

#[derive(Debug, Clone)]
pub struct Responder {
	pub addr: SocketAddr,
	pub last_response: DnsResponse,
	pub last_responded: Instant,
}

#[derive(Clone)]
pub(super) struct ResponderMemoryEntry {
	pub(super) inner: Arc<Responder>,
	pub(super) ignored_packets: Cell<u8>,
}
impl Deref for ResponderMemoryEntry {
	type Target = Responder;

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}
impl Borrow<SocketAddr> for ResponderMemoryEntry {
	fn borrow(&self) -> &SocketAddr {
		&self.addr
	}
}
impl Hash for ResponderMemoryEntry {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.addr.hash(state);
	}
}
impl PartialEq for ResponderMemoryEntry {
	fn eq(&self, other: &Self) -> bool {
		self.addr == other.addr
	}
}
impl Eq for ResponderMemoryEntry {}

#[derive(Default)]
pub(super) struct ResponderMemory(HashSet<ResponderMemoryEntry>);
impl ResponderMemory {
	#[inline(always)]
	pub(super) fn get(&self, addr: &SocketAddr) -> Option<&ResponderMemoryEntry> {
		self.0.get(addr)
	}

	#[inline(always)]
	pub(super) fn replace(&mut self, entry: Arc<Responder>) {
		self.0.replace(ResponderMemoryEntry {
			inner: entry,
			ignored_packets: Cell::new(0),
		});
	}

	pub(super) fn sweep(&mut self, event_handler: &EventHandler, max_ignored_packets: u8) {
		self.0.retain(|entry| {
			let ignored_packets = entry.ignored_packets.get();
			if ignored_packets < max_ignored_packets {
				entry.ignored_packets.set(ignored_packets + 1);
				true
			} else {
				let event_handler = event_handler.clone();
				let responder = entry.inner.clone();
				tokio::task::spawn_blocking(move || event_handler(DiscoveryEvent::ResponderLost(responder)));
				false
			}
		});
	}
}
