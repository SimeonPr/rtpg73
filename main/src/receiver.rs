//! Receives packets via the network
use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info, error};

use crate::messages;

pub fn run(manager_tx: cbc::Sender<messages::Manager>) {
    debug!("Receiver up and running...");
    let addr: SocketAddr = "0.0.0.0:4567".parse().expect("address should be parseable");

    let socket = UdpSocket::bind(addr).expect("socket should be bindable");
    info!("Listening on {}", socket.local_addr().expect("local_addr should be retrievable"));

    let mut buf = [0u8; 1024];

    loop {
        debug!("Ready for input...");
        match socket.recv_from(&mut buf) {
            Err(e) => {
                error!("receive from network failed: {}", e);
                continue;
            },
            _ => ()
        }
        match bincode::deserialize::<messages::Manager>(&buf) {
            Ok(deserialized) => manager_tx.send(deserialized).expect("message should be sendable"),
            Err(e) => {
                error!("received malformatted packet: {e}");
                continue;
            }
        }
    }
}
