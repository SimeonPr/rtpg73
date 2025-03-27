//! Network Sender Module
//!
//! Handles outgoing broadcast communications for the elevator control system.
//! Listens for manager messages and broadcasts them to the network.
use core::net::SocketAddr;
use std::net::UdpSocket;

use crossbeam_channel as cbc;
use log::{debug, info, error};

use crate::messages;
use bincode;

/// Main sender loop that broadcasts manager messages via UDP
///
/// # Parameters
/// - `rx`: Channel receiver for messages from the manager
///
/// # Behavior
/// 1. Binds to a random available UDP port
/// 2. Configures socket for broadcast (255.255.255.255:4567)
/// 3. Continuously listens for messages from manager
/// 4. Serializes and broadcasts valid messages
/// 5. Logs and skips network errors
///
/// # Network Configuration
/// - Uses UDP broadcast on port 4567
/// - Binds to any available local port (0.0.0.0:0)
/// - Requires broadcast capability
///
/// # Error Handling
/// - Continues operation after network errors
/// - Logs errors for failed broadcasts
/// - Panics only on initial setup failures (bind, broadcast enable)
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
