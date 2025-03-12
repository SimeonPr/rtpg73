use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info};

use crate::messages;
use bincode;

pub fn run(rx: cbc::Receiver<messages::Manager>) {
    debug!("Sender up and running...");
    let addr: SocketAddr = "0.0.0.0:0".parse().expect("couldn't parse address");
    let destination_addr: SocketAddr = "0.0.0.0:4567".parse().expect("couldn't parse address");
    let socket = UdpSocket::bind(addr).expect("couldn't bind");

    info!("Sending on {}", socket.local_addr().expect("local_addr failed"));

    loop {
        debug!("Waiting for input...");
        cbc::select! {
            recv(rx) -> a => {
                let packet = a.expect("couldn't get message");
                let serialized = bincode::serialize(&packet).expect("serialize failed");
                socket.send_to(&serialized, destination_addr).expect("send_to failed");
            }
        }        
    }
}
