use std::net::{Ipv4Addr, Ipv6Addr};

#[macro_use]
extern crate thiserror;

mod broadcast;
mod discovery;
mod socket;
mod util;

pub mod errors;

pub use broadcast::{
    Broadcaster, BroadcasterBuilder, BroadcasterHandle, IntoServiceTxt, Service, ServiceBuilder,
};

pub use discovery::{Discovery, DiscoveryBuilder, DiscoveryEvent, DiscoveryHandle, Responder};

pub const MDNS_PORT: u16 = 5353;
pub const MDNS_V4_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_V6_IP: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0xfb);

pub mod dns {
    pub use trust_dns_client::{
        self, op::DnsResponse, rr::DNSClass as DnsClass, rr::IntoName as IntoDnsName,
        rr::Name as DnsName, rr::RecordType as DnsRecordType,
    };
}

pub mod net {
    pub use crate::util::iface_v6_name_to_index;
    pub use if_addrs;
}

#[cfg(test)]
mod tests;
