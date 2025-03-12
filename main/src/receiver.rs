use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info};

use crate::messages;

pub fn run(manager_tx: cbc::Sender<messages::Manager>) {
    debug!("Receiver up and running...");
    let addr: SocketAddr = "0.0.0.0:4567".parse().expect("couldn't parse addr");

    let socket = UdpSocket::bind(addr).expect("couldn't bind socket");
    info!("Listening on {}", socket.local_addr().expect("local_addr failed"));

    let mut buf = [0u8; 1024];

    loop {
        debug!("Ready for input...");
        let (_, _) = socket.recv_from(&mut buf).expect("recv_from failed");
        // Deserialize the binary data back to a struct
        let deserialized: messages::Manager = bincode::deserialize(&buf).expect("deserialize failed");
        manager_tx.send(deserialized).expect("couldn't send to manager");
    }
}
