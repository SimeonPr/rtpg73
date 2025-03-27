//! Network Receiver Module
//!
//! Handles incoming UDP network communications for the elevator control system.
//! Listens for messages, deserializes them, and forwards to the manager.
use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info, error};

use crate::messages;

/// Main receiver loop that listens for and processes incoming network messages
///
/// # Parameters
/// - `manager_tx`: Channel sender for forwarding received messages to the manager
///
/// # Behavior
/// 1. Binds to UDP port 4567 on all interfaces
/// 2. Continuously listens for incoming datagrams
/// 3. Attempts to deserialize received data into Manager messages
/// 4. Forwards valid messages to the manager via channel
/// 5. Logs and skips malformed messages
///
/// # Error Handling
/// - Continues operation after network errors
/// - Logs errors for malformed packets
/// - Panics only on initial setup failures (bind)
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
