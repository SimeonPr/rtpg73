//! Receives events and drives state machine
use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use log::debug;
use log::info;

use crate::messages;
use crate::fsm;
use std::thread::spawn;
use std::time::Duration;
/// A loop processing hardware events, communicating with the manager and driving the elevator state machine. It does not handle call button presses.
pub fn run(controller_rx: cbc::Receiver<messages::Controller>, manager_tx: cbc::Sender<messages::Manager>, elevator_connection: e::Elevator) -> std::io::Result<()> {
    info!("Controller up and running.");
    let (timer_tx, timer_rx) = cbc::unbounded::<bool>();
    let mut elevator_state = fsm::ElevatorState::init_elevator(elevator_connection.clone(), timer_tx);

    let poll_period = Duration::from_millis(25);

    info!("Starting hardware monitors.");
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

    while elevator_connection.floor_sensor().is_none() {}
    
    loop {
        cbc::select! {
            recv(controller_rx) -> a => {
                let message = a.expect("controller couldn't receive");
                match message {
                    messages::Controller::Requests(requests) => {
                        debug!("Received Requests");
                        elevator_state.fsm_on_new_requests(requests, &manager_tx);
                    }
                }
            },
            recv(floor_sensor_rx) -> a => {
                let floor_sensor = a.expect("floor_sensor couldn't receive");
                debug!("Received FloorSensor");
                elevator_state.fsm_on_floor_arrival(floor_sensor as i8, &manager_tx);
            },
            recv(stop_button_rx) -> a => {
                debug!("Received StopButton");
                let _stop_button = a.expect("stop_button couldn't receive");
                elevator_state.fsm_on_stop_button_press();
            },
            recv(obstruction_rx) -> a => {
                debug!("Received Obstruction");
                let obstruction = a.expect("obstruction couldn't receive");
                elevator_state.fsm_on_obstruction(obstruction);
            },
            recv(timer_rx) -> a => {
                let _time_out = a.expect("timer couldn't receive");
                elevator_state.fsm_on_door_time_out(&manager_tx);
            }
        };
    }
}

