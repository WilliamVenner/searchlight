use searchlight::{
    broadcast::{BroadcasterBuilder, ServiceBuilder},
    discovery::{DiscoveryBuilder, DiscoveryEvent},
};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

#[test]
fn client_and_server() {
    let (test_tx, test_rx) = std::sync::mpsc::sync_channel(0);

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);

        println!("Starting server");

        let server = Arc::new(Mutex::new(Some(
            BroadcasterBuilder::new()
                .loopback()
                .add_service(
                    ServiceBuilder::new("_searchlight-test._udp.local", "searchlighttest", 1337)
                        .unwrap()
                        .add_ip_address(IpAddr::V4(Ipv4Addr::from_str("192.168.1.69").unwrap()))
                        .add_ip_address(IpAddr::V6(
                            Ipv6Addr::from_str("fe80::18e4:b943:8756:d855").unwrap(),
                        ))
                        .add_txt("key=value")
                        .add_txt_truncate("key2=value2")
                        .build()
                        .unwrap(),
                )
                .build()
                .expect("Failed to create mDNS broadcaster")
                .run_background(),
        )));

        println!("Server is running");

        println!("Starting client");

        let server_ref = server.clone();
        let client = DiscoveryBuilder::new()
            .service("_searchlight-test._udp.local")
            .unwrap()
            .loopback()
            .any_ip()
            .build()
            .unwrap()
            .run_background(move |event| {
                if let DiscoveryEvent::ResponderFound(responder)
                | DiscoveryEvent::ResponderLost(responder) = &event
                {
                    println!(
                        "Got {} from server with names {:?}",
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
                            .collect::<BTreeSet<_>>()
                    );

                    let is_test_responder =
                        responder.last_response.answers().iter().any(|answer| {
                            answer.name().to_string()
                                == "searchlighttest._searchlight-test._udp.local."
                        });

                    if is_test_responder {
                        if matches!(&event, DiscoveryEvent::ResponderFound(_)) {
                            println!("Got ResponderFound from server");
                            // Shut down the server so we can get a ResponderLost event
                            if let Some(server) = server_ref.lock().unwrap().take() {
                                server.shutdown().unwrap().unwrap();
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

        test_tx.send(()).ok();
    });

    test_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("Timed out waiting for test to finish");
}
