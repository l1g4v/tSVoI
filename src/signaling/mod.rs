// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

pub mod client;
pub mod server;

use std::net::ToSocketAddrs;
use std::net::UdpSocket;
use stunclient::StunClient;

pub fn get_address_ipv6() -> String {
    let stun_addr = "stun.l.google.com:19302"
        .to_socket_addrs()
        .unwrap()
        .filter(|x| x.is_ipv6())
        .next()
        .unwrap();
    let udp = UdpSocket::bind("[::]:0").unwrap();
    let mut c = StunClient::new(stun_addr);
    c.set_software(Some("tSVoI"));
    c.query_external_address(&udp).unwrap().to_string()
}
pub fn get_address_ipv4() -> String {
    let stun_addr = "stun.l.google.com:19302"
        .to_socket_addrs()
        .unwrap()
        .filter(|x| x.is_ipv4())
        .next()
        .unwrap();
    let udp = UdpSocket::bind("[::]:0").unwrap();
    let mut c = StunClient::new(stun_addr);
    c.set_software(Some("tSVoI"));
    c.query_external_address(&udp).unwrap().to_string()
}
