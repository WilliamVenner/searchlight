use crate::broadcast::ServiceBuilder;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};
use trust_dns_client::serialize::binary::{BinEncodable, BinEncoder};

#[test]
fn test_dns_parser_backwards_compatibility() {
    let dns_message = ServiceBuilder::new("_venner-test._udp.local", "helloworld", 1337)
        .unwrap()
        .add_ip_address(IpAddr::V4(Ipv4Addr::from_str("192.168.1.69").unwrap()))
        .add_ip_address(IpAddr::V6(
            Ipv6Addr::from_str("fe80::18e4:b943:8756:d855").unwrap(),
        ))
        .add_txt("key=value")
        .add_txt_truncated("key2=value2")
        .build()
        .unwrap()
        .dns_response()
        .unwrap();

    println!("========== OURS ==========\n{dns_message:#?}\n");

    let mut buf = Vec::with_capacity(4096);
    dns_message.emit(&mut BinEncoder::new(&mut buf)).unwrap();

    println!(
        "========== THEIRS ==========\n{:#?}",
        dns_parser::Packet::parse(&buf).unwrap()
    );
}
