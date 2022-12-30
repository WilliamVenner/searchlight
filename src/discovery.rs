//! # mDNS Discovery
//!
//! This module provides a way to discover other mDNS responders on the network.
//!
//! In other words, this module provides an _mDNS client_.
//!
//! # Example
//!
//! ```rust, no_run
//! use searchlight::{
//!     discovery::{DiscoveryBuilder, DiscoveryEvent},
//!     dns::{op::DnsResponse, rr::RData},
//!     net::IpVersion,
//! };
//!
//! fn get_chromecast_name(dns_packet: &DnsResponse) -> String {
//!     dns_packet
//!         .additionals()
//!         .iter()
//!         .find_map(|record| {
//!             if let Some(RData::SRV(_)) = record.data() {
//!                 let name = record.name().to_utf8();
//!                 let name = name.strip_suffix('.').unwrap_or(&name);
//!                 let name = name.strip_suffix("_googlecast._tcp.local").unwrap_or(&name);
//!                 let name = name.strip_suffix('.').unwrap_or(&name);
//!                 Some(name.to_string())
//!             } else {
//!                 None
//!             }
//!         })
//!         .unwrap_or_else(|| "Unknown".into())
//! }
//!
//! DiscoveryBuilder::new()
//!     .service("_googlecast._tcp.local.")
//!     .unwrap()
//!     .build(IpVersion::Both)
//!     .unwrap()
//!     .run(|event| match event {
//!         DiscoveryEvent::ResponderFound(responder) => {
//!             println!(
//!                 "Found Chromecast {} at {}",
//!                 get_chromecast_name(&responder.last_response),
//!                 responder.addr.ip()
//!             );
//!         }
//!
//!         DiscoveryEvent::ResponderLost(responder) => {
//!             println!(
//!                 "Chromecast {} at {} has gone away",
//!                 get_chromecast_name(&responder.last_response),
//!                 responder.addr.ip()
//!             );
//!         }
//!
//!         DiscoveryEvent::ResponseUpdate { .. } => {}
//!     })
//!     .unwrap();
//! ```

use crate::socket::{AsyncMdnsSocket, MdnsSocket};
use std::{
	net::SocketAddr,
	sync::Arc,
	time::{Duration, Instant},
};
use trust_dns_client::{
	op::{DnsResponse, Message as DnsMessage, MessageType as DnsMessageType, Query as DnsQuery},
	rr::{DNSClass as DnsClass, Name as DnsName, RecordType as DnsRecordType},
	serialize::binary::{BinDecodable, BinEncodable},
};

mod builder;
pub use builder::DiscoveryBuilder;

mod event;
pub use event::DiscoveryEvent;
use event::*;

mod handle;
pub use handle::DiscoveryHandle;
use handle::*;

mod presence;
pub use presence::Responder;
use presence::*;

fn discovery_packet(unicast: bool, service_name: Option<&DnsName>) -> Result<Vec<u8>, std::io::Error> {
	DnsMessage::new()
		.add_query({
			let mut query = DnsQuery::new();

			if let Some(service_name) = service_name {
				query.set_name(service_name.clone());
			}

			query
				.set_query_type(DnsRecordType::PTR)
				.set_query_class(DnsClass::IN)
				.set_mdns_unicast_response(unicast);

			query
		})
		.to_bytes()
		.map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, format!("Discovery packet failed to serialize: {err}")))
}

/// A built mDNS discovery (client) instance, ready to be started.
///
/// You can choose to run discovery on the current thread, or in the background, using [`Discovery::run`] or [`Discovery::run_in_background`].
///
/// A `Discovery` instance can be built using [`DiscoveryBuilder`].
pub struct Discovery {
	socket: MdnsSocket,
	service_name: Option<DnsName>,
	interval: Duration,
	max_ignored_packets: u8,
}
impl Discovery {
	/// Returns a new [`DiscoveryBuilder`].
	pub fn builder() -> DiscoveryBuilder {
		DiscoveryBuilder::new()
	}

	/// Run discovery on a new thread; in the background.
	///
	/// Returns a [`DiscoveryHandle`] that can be used to cleanly shut down the background thread.
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

		DiscoveryHandle(DiscoveryHandleDrop(Some(DiscoveryHandleInner { thread, shutdown_tx })))
	}

	/// Run discovery on the current thread.
	///
	/// This will start a new Tokio runtime on the current thread and block until a fatal error occurs.
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
	async fn impl_run(self, handler: EventHandler, shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>) -> Result<(), std::io::Error> {
		let Discovery {
			socket,
			service_name,
			interval,
			max_ignored_packets,
		} = self;

		let socket = socket.into_async().await?;

		let shutdown = async move {
			if let Some(shutdown_rx) = shutdown_rx {
				shutdown_rx.await
			} else {
				std::future::pending().await
			}
		};

		tokio::select! {
			biased;
			res = Self::discovery_loop(handler, service_name, interval, max_ignored_packets, &socket) => res,
			_ = shutdown => Ok(()),
		}
	}

	async fn discovery_loop(
		event_handler: EventHandler,
		service_name: Option<DnsName>,
		discovery_interval: Duration,
		max_ignored_packets: u8,
		socket: &AsyncMdnsSocket,
	) -> Result<(), std::io::Error> {
		let service_name = service_name.as_ref();

		// Response listening
		let mut socket_recv = socket.recv(vec![0; 4096]);

		// Discovery
		let discovery_packet = discovery_packet(false, service_name)?;
		let mut discovery_interval = tokio::time::interval(discovery_interval);
		discovery_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

		// Presence
		let mut responder_memory = ResponderMemory::default();

		loop {
			tokio::select! {
				biased; // Prefer handling packets
				recv = socket_recv.recv_multicast() => {
					let recv = recv?;
					Self::recv_multicast(service_name, &event_handler, &mut responder_memory, recv).await;
				}

				_ = discovery_interval.tick() => {
					// Send discovery packet!
					socket.send_multicast(&discovery_packet).await?;

					if max_ignored_packets == 0 {
						continue;
					}

					// Give responders a chance to respond
					let mut deadline = tokio::time::Instant::now() + Duration::from_secs(2);
					loop {
						let recv = match tokio::time::timeout_at(deadline, socket_recv.recv_multicast()).await {
							Ok(Ok(recv)) => recv,
							Ok(Err(err)) => return Err(err),
							Err(_) => break,
						};

						let forgiveness = tokio::time::Instant::now();
						Self::recv_multicast(service_name, &event_handler, &mut responder_memory, recv).await;
						deadline += forgiveness.elapsed(); // Add the time we spent processing the packet to the deadline
					}

					// Remove stale responders
					responder_memory.sweep(&event_handler, max_ignored_packets);
				}
			}
		}
	}

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

		let response = match DnsMessage::from_bytes(&packet[..count]) {
			Ok(response) if response.message_type() == DnsMessageType::Response => DnsResponse::from(response),
			_ => return,
		};

		if let Some(service_name) = service_name {
			if !response.answers().iter().any(|answer| answer.name() == service_name) {
				// This response does not contain the service we are looking for.
				return;
			}
		}

		let event = {
			let old = response_memory_bank.get(&addr).map(|response_memory| response_memory.inner.clone());

			let new = {
				let responder = Arc::new(Responder {
					addr,
					last_response: response,
					last_responded: Instant::now(),
				});
				response_memory_bank.replace(responder.clone());
				responder
			};

			match old {
				Some(old) => DiscoveryEvent::ResponseUpdate { old, new },
				None => DiscoveryEvent::ResponderFound(new),
			}
		};

		let event_handler = event_handler.clone();
		tokio::task::spawn_blocking(move || event_handler(event)).await.ok();
	}
}
