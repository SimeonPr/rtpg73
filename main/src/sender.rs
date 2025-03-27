//! Sends outgoing messages via the network
use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info, error};

use crate::messages;
use bincode;

pub fn run(
    rx: cbc::Receiver<messages::Manager>,
    manager_tx: cbc::Sender<messages::Manager>
) {
    debug!("Sender up and running...");
    let addr: SocketAddr = "0.0.0.0:0".parse().expect("address should be parseable");
    let destination_addr: SocketAddr = "255.255.255.255:4567".parse().expect("address should be parseable");
    let socket = UdpSocket::bind(addr).expect("address needs to be available");
    socket.set_broadcast(true).expect("broadcast is essential for communication");
    info!("Sending on {}", socket.local_addr().expect("local_addr should not failf"));
    loop {
        debug!("Waiting for input...");
        cbc::select! {
            recv(rx) -> a => {
                let packet = a.expect("message should be unwrappable");
                let serialized = bincode::serialize(&packet).expect("serialization should work");
                for tries in 0..5 {
                    let res = socket.send_to(&serialized, destination_addr);
                    match res {
                        Err(e) => {
                            if tries == 4 {
                                error!("broadcast on network failed: {}", e);
                                manager_tx.send(messages::Manager::NetworkError).expect("channel should work");
                                continue;
                            }
                        },
                        _ => break
                    }
                }
            }
        }        
    }
}
