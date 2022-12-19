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

pub(super) struct ResponderMemory {
	pub memory: HashSet<ResponderMemoryEntry>,
}
impl ResponderMemory {
	pub(super) fn get(&self, addr: &SocketAddr) -> Option<&ResponderMemoryEntry> {
		self.memory.get(addr)
	}

	pub(super) fn replace(&mut self, entry: Arc<Responder>) {
		self.memory.replace(ResponderMemoryEntry {
			inner: entry,
			ignored_packets: Cell::new(0),
		});
	}
}
