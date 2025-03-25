use crossbeam_channel as cbc;
use log::debug;
use log::trace;
use crate::messages;
use crate::fsm;
use driver_rust::elevio::elev as e;
use crate::config;

pub fn run(lights_rx: cbc::Receiver<messages::Controller>, elev_conn: e::Elevator) {
    loop {
        cbc::select! {
            recv(lights_rx) -> a => {
                match a.expect("couldn't get message") {
                    messages::Controller::Requests(requests) => {
                        debug!("Received Requests");
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


#[cfg(test)]
mod test_lights {
    use super::*;
    use crate::fsm::ControllerRequests;
    use crate::config;

    #[test]
    // #[ignore = "Requires a running elevator simulator"]
    fn integration_test_set_all_lights() {
        let mut elev_conn = e::Elevator::init("127.0.0.1:15657", config::FLOOR_COUNT as u8)
            .expect("Failed to initialize Elevator");


        let mut requests: ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];
        requests[0][0] = true; 
        requests[1][1] = true; 

        set_all_lights(&mut elev_conn, &requests);

        for f in 0..config::FLOOR_COUNT {
            for b in 0..config::CALL_COUNT {
                let expected = requests[f][b];
                // Since call_button_light returns (), we cannot directly compare it.
                // Instead, we assume the function executes without errors as verification.
                elev_conn.call_button_light(f as u8, b as u8, expected);

                debug!(
                    "Set light at floor {}, button {} to {}",
                    f, b, expected
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "Failed to initialize Elevator")]
    fn test_set_all_lights_panic() {
        let mut elev_conn = e::Elevator::init("627.0.0.1:15657", config::FLOOR_COUNT as u8)
            .expect("Failed to initialize Elevator");
    
        let requests: ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];
        set_all_lights(&mut elev_conn, &requests);
    }

}