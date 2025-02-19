use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info};

use crate::messages;
use serde::{Serialize, Deserialize};
use bincode;

pub fn run(rx: cbc::Receiver<messages::Network>) {
    debug!("Sender up and running...");
    let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let destination_addr: SocketAddr = "0.0.0.0:4567".parse().unwrap();
    let socket = UdpSocket::bind(addr).unwrap();

    info!("Sending on {}", socket.local_addr().unwrap());

    loop {
        debug!("Waiting for input...");
        cbc::select! {
            recv(rx) -> a => {
                let packet = a.unwrap();
                let serialized = bincode::serialize(&packet).unwrap();
                socket.send_to(&serialized, destination_addr).unwrap();
            }
        }        
    }
}
