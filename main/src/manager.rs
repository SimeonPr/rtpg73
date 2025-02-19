use crossbeam_channel as cbc;
use driver_rust::elevio;
use log::{debug, info};
use crate::messages;
pub fn run(manager_rx: cbc::Receiver<messages::Manager>,
    sender_tx: cbc::Sender<messages::Network>,
    controller_tx: cbc::Sender<messages::Controller>,
    call_button_rx: cbc::Receiver<elevio::poll::CallButton>) {
        debug!("Manager up and running...");
        loop {
            debug!("Waiting for input...");
            cbc::select! {
                recv(manager_rx) -> a => {
                    let message = a.unwrap();
                    match message {
                        messages::Manager::Ping => {
                            info!("Received ping");
                        },
                    }
                }
            }
        }
    }
