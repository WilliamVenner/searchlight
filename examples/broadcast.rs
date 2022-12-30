use searchlight::{
	broadcast::{BroadcasterBuilder, ServiceBuilder},
	discovery::{DiscoveryBuilder, DiscoveryEvent},
	net::IpVersion,
};
use std::{
	net::{IpAddr, Ipv4Addr},
	str::FromStr,
};

fn main() {
	let (found_tx, found_rx) = std::sync::mpsc::sync_channel(0);

	let broadcaster = BroadcasterBuilder::new()
		.loopback()
		.add_service(
			ServiceBuilder::new("_searchlight._udp.local.", "HELLO-WORLD", 1234)
				.unwrap()
				.add_ip_address(IpAddr::V4(Ipv4Addr::from_str("192.168.1.69").unwrap()))
				.add_txt_truncated("key=value")
				.add_txt_truncated("key2=value2")
				.build()
				.unwrap(),
		)
		.build(IpVersion::V4)
		.unwrap()
		.run_in_background();

	let discovery = DiscoveryBuilder::new()
		.loopback()
		.service("_searchlight._udp.local.")
		.unwrap()
		.build(IpVersion::V4)
		.unwrap()
		.run_in_background(move |event| {
			if let DiscoveryEvent::ResponderFound(responder) = event {
				found_tx.try_send(responder).ok();
			}
		});

	println!("Waiting for discovery to find responder...");

	println!("{:#?}", found_rx.recv().unwrap());

	println!("Shutting down...");

	broadcaster.shutdown().unwrap();
	discovery.shutdown().unwrap();

	println!("Done!");
}
