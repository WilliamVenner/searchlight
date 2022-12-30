<p align="center">
	<a href="https://crates.io/crates/searchlight"><img alt="crates.io" src="https://img.shields.io/crates/v/searchlight.svg"/></a>
	<a href="https://docs.rs/searchlight/"><img alt="docs.rs" src="https://docs.rs/searchlight/badge.svg"/></a>
	<img alt="License" src="https://img.shields.io/crates/l/searchlight"/>
</p>

<h1 align="center">📡 Searchlight</h1>

Searchlight is an mDNS server & client library designed to be simple, lightweight and easy to use,
even if you just have basic knowledge about mDNS.

In layman's terms, Searchlight is a library for broadcasting and discovering "services" on a local network.
This technology is part of the same technology used by Chromecast, AirDrop, Phillips Hue, and et cetera.

**Searchlight is designed with user interfaces in mind.**
The defining feature of this library is that it keeps track of the presence of services on the network,
and notifies you when they come and go, allowing you to update your user interface accordingly,
providing a user experience that is responsive, intuitive and familiar to a scanning list for
WiFi, Bluetooth, Chromecast, etc.

- **🌐 IPv4 and IPv6** - Support for both IPv4 and IPv6.
- **✨ OS support** - Support for Windows, macOS and most UNIX systems.
- **📡 Broadcasting** - Send out service announcements to the network and respond to discovery requests. (mDNS server)
- **👽 Discovery** - Discover services on the network and keep track of their presence. (mDNS client)
- **🧵 Single threaded** - Searchlight operates on just a single thread, thanks to the [Tokio](https://tokio.rs/) async runtime & task scheduler.
- **🤸 Flexible API** - No async, no streams, no channels, no bullsh*t. Just provide an event handler function and bridge the gap between your application and Searchlight however you like.
- **👻 Background runtime** - Discovery and broadcasting can both run in the background on separate threads, providing a handle to gracefully shut down if necessary.
- **📨 UDP** - All networking, including discovery and broadcasting, is connectionless and done over UDP.
- **🔁 Loopback** - Support for receiving packets sent by the same socket, intended to be used in tests.
- **🎯 Interface targeting** - Support for targeting a specific network interface(s) for discovery and broadcasting.

# Usage

Add Searchlight to your [`Cargo.toml`](https://doc.rust-lang.org/cargo/reference/manifest.html) file:

```toml
[dependencies]
searchlight = "0.2.0"
```

To learn more about how to use Searchlight, see the [documentation](https://docs.rs/searchlight/).

# Examples

## Discovery

Find all Chromecasts on the network.

```rust
use searchlight::{
    discovery::{DiscoveryBuilder, DiscoveryEvent},
    dns::{op::DnsResponse, rr::RData},
    net::IpVersion,
};

fn get_chromecast_name(dns_packet: &DnsResponse) -> String {
    dns_packet
        .additionals()
        .iter()
        .find_map(|record| {
            if let Some(RData::SRV(_)) = record.data() {
                let name = record.name().to_utf8();
                let name = name.strip_suffix('.').unwrap_or(&name);
                let name = name.strip_suffix("_googlecast._tcp.local").unwrap_or(&name);
                let name = name.strip_suffix('.').unwrap_or(&name);
                Some(name.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Unknown".into())
}

DiscoveryBuilder::new()
    .service("_googlecast._tcp.local.")
    .unwrap()
    .build(IpVersion::Both)
    .unwrap()
    .run(|event| match event {
        DiscoveryEvent::ResponderFound(responder) => {
            println!(
                "Found Chromecast {} at {}",
                get_chromecast_name(&responder.last_response),
                responder.addr.ip()
            );
        }
        DiscoveryEvent::ResponderLost(responder) => {
            println!(
                "Chromecast {} at {} has gone away",
                get_chromecast_name(&responder.last_response),
                responder.addr.ip()
            );
        }
        DiscoveryEvent::ResponseUpdate { .. } => {}
    })
    .unwrap();
```

## Broadcasting

Broadcast a service on the network, and verify that it can be discovered.

```rust
use searchlight::{
    broadcast::{BroadcasterBuilder, ServiceBuilder},
    discovery::{DiscoveryBuilder, DiscoveryEvent},
    net::IpVersion,
};
use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
};

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
```

# Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the MIT license, shall be dual licensed as above, without any additional terms or conditions.
