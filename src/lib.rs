use std::net::{Ipv4Addr, Ipv6Addr};

#[macro_use]
extern crate thiserror;

mod socket;
mod util;

pub mod broadcast;
pub mod discovery;
pub mod errors;
pub mod net;

pub const MDNS_PORT: u16 = 5353;
pub const MDNS_V4_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_V6_IP: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0xfb);

pub use trust_dns_client as dns;

#[cfg(test)]
mod tests;
