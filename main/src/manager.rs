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
                },
                recv(call_button_rx) -> a => {
                    let message = a.unwrap();
                    info!("Received button press");
                    debug!("{:?}", message);
                    // forward to controller
                    controller_tx.send(messages::Controller::Request(message)).unwrap();
                }
            }
        }
    }
