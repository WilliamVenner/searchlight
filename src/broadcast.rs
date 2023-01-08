//! # mDNS Broadcasting
//!
//! This module provides a way to respond to mDNS queries on the network.
//!
//! In other words, this module provides an _mDNS server_.
//!
//! # Example
//!
//! ```rust, no_run
//! use searchlight::{
//!     broadcast::{BroadcasterBuilder, ServiceBuilder},
//!     discovery::{DiscoveryBuilder, DiscoveryEvent},
//!     net::IpVersion,
//! };
//! use std::{
//!     net::{IpAddr, Ipv4Addr},
//!     str::FromStr,
//! };
//!
//! let (found_tx, found_rx) = std::sync::mpsc::sync_channel(0);
//!
//! let broadcaster = BroadcasterBuilder::new()
//!     .loopback()
//!     .add_service(
//!         ServiceBuilder::new("_searchlight._udp.local.", "HELLO-WORLD", 1234)
//!             .unwrap()
//!             .add_ip_address(IpAddr::V4(Ipv4Addr::from_str("192.168.1.69").unwrap()))
//!             .add_txt_truncated("key=value")
//!             .add_txt_truncated("key2=value2")
//!             .build()
//!             .unwrap(),
//!     )
//!     .build(IpVersion::V4)
//!     .unwrap()
//!     .run_in_background();
//!
//! let discovery = DiscoveryBuilder::new()
//!     .loopback()
//!     .service("_searchlight._udp.local.")
//!     .unwrap()
//!     .build(IpVersion::V4)
//!     .unwrap()
//!     .run_in_background(move |event| {
//!         if let DiscoveryEvent::ResponderFound(responder) = event {
//!             found_tx.try_send(responder).ok();
//!         }
//!     });
//!
//! println!("Waiting for discovery to find responder...");
//!
//! println!("{:#?}", found_rx.recv().unwrap());
//!
//! println!("Shutting down...");
//!
//! broadcaster.shutdown().unwrap();
//! discovery.shutdown().unwrap();
//!
//! println!("Done!");
//! ```

use crate::socket::{AsyncMdnsSocket, MdnsSocket, MdnsSocketRecv};
use std::{
	collections::BTreeSet,
	sync::{Arc, RwLock},
};
use trust_dns_client::{
	op::Message as DnsMessage,
	serialize::binary::{BinDecodable, BinEncodable, BinEncoder},
};

/// Errors that can occur while broadcasting or initializing a broadcaster.
pub mod errors;

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

/// A built mDNS broadcaster (server) instance, ready to be started.
///
/// You can choose to run broadcasting on the current thread, or in the background, using [`Broadcaster::run`] or [`Broadcaster::run_in_background`].
///
/// A `Broadcaster` can be built using [`BroadcasterBuilder`].
pub struct Broadcaster {
	socket: MdnsSocket,
	config: Arc<RwLock<BroadcasterConfig>>,
}
impl Broadcaster {
	/// Run broadcasting on a new thread; in the background.
	///
	/// Returns a [`BroadcasterHandle`] that can be used to cleanly shut down the background thread.
	pub fn run_in_background(self) -> BroadcasterHandle {
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
					Self::impl_run(&socket, socket.recv(vec![0; 4096]), config_ref, Some(shutdown_rx)).await
				})
		});

		BroadcasterHandle(BroadcasterHandleDrop(Some(BroadcasterHandleInner { config, thread, shutdown_tx })))
	}

	/// Run broadcasting on the current thread.
	///
	/// This will start a new Tokio runtime on the current thread and block until a fatal error occurs.
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
	async fn recv_loop(tx: &AsyncMdnsSocket, rx: &mut MdnsSocketRecv<'_>, config: &RwLock<BroadcasterConfig>) -> Result<(), std::io::Error> {
		let mut send_buf = vec![0u8; 4096];
		loop {
			let ((count, addr), packet) = match rx.recv_multicast().await {
				Ok(recv) => recv,
				Err(err) => {
					log::warn!("Failed to receive on mDNS socket: {err}");
					continue;
				}
			};
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

			for service in config.read().unwrap().services.iter().filter(|service| {
				if service.service_type() == query.name() {
					return true;
				}

				if let Some(subtype_suffix) = &service.service_subtype_suffix {
					if query.name().to_utf8().ends_with(subtype_suffix) {
						return true;
					}
				}

				false
			}) {
				send_buf.clear();

				if service.dns_response.emit(&mut BinEncoder::new(&mut send_buf)).is_ok() {
					if query.mdns_unicast_response() {
						// Send unicast packet
						if let Err(err) = tx.send_to(&send_buf, addr).await {
							log::warn!("Failed to send unicast mDNS response to {addr}: {err}");
						}
					} else {
						// Send multicast packet
						if let Err(err) = tx.send_multicast(&send_buf).await {
							log::warn!("Failed to send multicast mDNS response (requested by {addr}): {err}");
						}
					}
				}
			}
		}
	}
}
