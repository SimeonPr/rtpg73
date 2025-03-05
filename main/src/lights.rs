use crossbeam_channel as cbc;
use log::{trace, debug, info};
use crate::messages;
use crate::fsm;
use driver_rust::elevio::elev as e;
use crate::config;

pub fn run(lights_rx: cbc::Receiver<messages::Controller>, elev_conn: e::Elevator) {
    loop {
        cbc::select! {
            recv(lights_rx) -> a => {
                match a.unwrap() {
                    messages::Controller::Requests(requests) => {
                        info!("Received Requests");
                        debug!("{:?}", &requests);
                        set_all_lights(&elev_conn, &requests);
                    }
                }
            }
        }
    }
}
fn set_all_lights(elev_conn: &e::Elevator, requests: &fsm::ControllerRequests) {
    trace!("set_all_lights");
    for f in 0..config::FLOOR_COUNT {
        for b in 0..config::CALL_COUNT {
            elev_conn.call_button_light(f as u8, b as u8, requests[f as usize][b as usize]);
        }
    }
}
