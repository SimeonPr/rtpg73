use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use log::{debug, info};

use crate::messages;
use crate::fsm;
use std::thread::spawn;
use std::time::Duration;

pub fn run(controller_rx: cbc::Receiver<messages::Controller>, manager_tx: cbc::Sender<messages::Manager>, elevator_connection: e::Elevator) -> std::io::Result<()> {
    debug!("Controller up and running.");
    let (timer_tx, timer_rx) = cbc::unbounded::<bool>();
    let mut elevator_state = fsm::ElevatorState::init_elevator(elevator_connection.clone(), timer_tx);

    let poll_period = Duration::from_millis(25);

    debug!("Starting hardware monitors.");
    let (floor_sensor_tx, floor_sensor_rx) = cbc::unbounded::<u8>();
    {
        let elevator = elevator_connection.clone();
        spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, poll_period));
    }

    let (stop_button_tx, stop_button_rx) = cbc::unbounded::<bool>();
    {
        let elevator = elevator_connection.clone();
        spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, poll_period));
    }

    let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>();
    {
        let elevator = elevator_connection.clone();
        spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, poll_period));
    }
    if elevator_connection.floor_sensor().is_none() {
        elevator_state.fsm_on_init_between_floors();
    }

    loop {
        debug!("Waiting for input.");
        cbc::select! {
            recv(controller_rx) -> a => {
                let message = a.unwrap();
                match message {
                    messages::Controller::Ping => {
                        info!("Received ping");
                    },
                }
            },
            recv(floor_sensor_rx) -> a => {
                let floor_sensor = a.unwrap();
                elevator_state.fsm_on_floor_arrival(floor_sensor as i8);
            },
            recv(stop_button_rx) -> a => {
                let _stop_button = a.unwrap();
                elevator_state.fsm_on_stop_button_press();
            },
            recv(obstruction_rx) -> a => {
                let obstruction = a.unwrap();
                elevator_state.fsm_on_obstruction(obstruction);
            },
            recv(timer_rx) -> a => {
                let _time_out = a.unwrap();
                elevator_state.fsm_on_door_time_out();
            }
        };
    }
}

