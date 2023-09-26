use searchlight::{
	broadcast::{BroadcasterBuilder, ServiceBuilder},
	discovery::{DiscoveryBuilder, DiscoveryEvent},
	net::{IpVersion, Ipv6Interface, TargetInterface},
};
use std::{
	collections::BTreeSet,
	net::{IpAddr, SocketAddr, UdpSocket},
	num::NonZeroU32,
	sync::{Arc, Mutex},
	time::Duration,
};

#[test]
fn client_and_server() {
	simple_logger::init_with_level(log::Level::Info).ok();

	let (test_tx, test_rx) = std::sync::mpsc::sync_channel(0);

	std::thread::spawn(move || {
		for (ip_version_name, ip_version) in [("IPv6", IpVersion::V6), ("IPv4", IpVersion::V4)] {
			let interface_addr = match ip_version {
				IpVersion::V4 => UdpSocket::bind("0.0.0.0:0").and_then(|socket| {
					socket.connect("1.1.1.1:53")?;
					socket.local_addr()
				}),
				IpVersion::V6 => UdpSocket::bind("[::]:0").and_then(|socket| {
					socket.connect("[2606:4700:4700::1111]:53")?;
					socket.local_addr()
				}),
				_ => unreachable!(),
			};

			let interface_addr = match interface_addr {
				Ok(addr) => {
					println!("Testing on interface {}", addr.ip());
					addr
				}
				Err(err) => {
					println!("Skipping {ip_version_name} test: {err:?}");
					continue;
				}
			};

			let service = ServiceBuilder::new("_searchlight-test._udp.local", "searchlighttest", 1337)
				.unwrap()
				.add_txt("key=value")
				.add_txt_truncated("key2=value2");

			let mut server = BroadcasterBuilder::new().loopback();

			let mut client = DiscoveryBuilder::new()
				.service("_searchlight-test._udp.local")
				.unwrap()
				.loopback()
				.interval(Duration::from_secs(1));

			(server, client) = match ip_version {
				IpVersion::V4 => {
					let target = TargetInterface::Specific(match interface_addr.ip() {
						IpAddr::V4(v4) => v4,
						_ => unreachable!(),
					});
					(server.interface_v4(target.clone()), client.interface_v4(target))
				}

				IpVersion::V6 => {
					let target = TargetInterface::Specific(
						Ipv6Interface::from_addr(&match interface_addr.ip() {
							IpAddr::V6(v6) => v6,
							_ => unreachable!(),
						})
						.unwrap(),
					);
					(server.interface_v6(target.clone()), client.interface_v6(target))
				}

				_ => unreachable!(),
			};

			let server = Arc::new(Mutex::new(Some(
				server
					.add_service(service.add_ip_address(interface_addr.ip()).build().unwrap())
					.build(ip_version)
					.expect("Failed to create mDNS broadcaster")
					.run_in_background(),
			)));

			let (tx, rx) = std::sync::mpsc::sync_channel(0);
			let server_ref = server.clone();
			let client = client.build(ip_version).unwrap().run_in_background(move |event| {
				if let DiscoveryEvent::ResponderFound(responder) | DiscoveryEvent::ResponderLost(responder) = &event {
					println!(
						"Got {} from server with names {:?} and address {:?}",
						match event {
							DiscoveryEvent::ResponderFound(_) => "ResponderFound",
							DiscoveryEvent::ResponderLost(_) => "ResponderLost",
							_ => unreachable!(),
						},
						responder
							.last_response
							.answers()
							.iter()
							.map(|answer| answer.name().to_string())
							.collect::<BTreeSet<_>>(),
						responder.addr
					);

					let is_test_responder = responder
						.last_response
						.additionals()
						.iter()
						.any(|answer| answer.name().to_string() == "searchlighttest._searchlight-test._udp.local.");

					if is_test_responder {
						// Check if this is the address we expected
						match (responder.addr, interface_addr) {
							(SocketAddr::V6(addr_v6), SocketAddr::V6(iface_v6)) => {
								assert!(Ipv6Interface::from_raw(NonZeroU32::new(addr_v6.scope_id()).unwrap())
									.addrs()
									.unwrap()
									.contains(iface_v6.ip()));
							}

							(SocketAddr::V4(addr_v4), SocketAddr::V4(iface_v4)) => {
								assert_eq!(addr_v4.ip(), iface_v4.ip());
							}

							_ => assert!(false),
						}

						if matches!(&event, DiscoveryEvent::ResponderFound(_)) {
							println!("Got ResponderFound from server");
							// Shut down the server so we can get a ResponderLost event
							if let Some(server) = server_ref.lock().unwrap().take() {
								server.shutdown().unwrap();
							}
						} else if matches!(&event, DiscoveryEvent::ResponderLost(_)) {
							println!("Got ResponderLost from server");
							// We're done here
							tx.try_send(()).ok();
						}
					}
				}
			});

			println!("Client is running");

			let res = rx.recv_timeout(Duration::from_secs(30));

			println!("Shutting down server");
			if let Some(server) = server.lock().unwrap().take() {
				println!("Server status: {:?}", server.shutdown());
			} else {
				println!("Server status: Shutdown");
			}

			println!("Shutting down client");
			println!("Client status: {:?}", client.shutdown());

			res.expect("Timed out waiting for server to respond");
		}

		test_tx.send(()).ok();
	});

	test_rx
		.recv_timeout(Duration::from_secs(30))
		.expect("Timed out waiting for test to finish");
}
