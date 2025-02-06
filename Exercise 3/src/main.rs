use std::thread::*;
use env_logger;

use crossbeam_channel as cbc;
use std::time::Duration;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;

mod fsm;

fn main() -> std::io::Result<()> {
    env_logger::init();
    let elev_num_floors = 4;
    let elevator_connection = e::Elevator::init("localhost:15657", elev_num_floors)?;
    let (timer_tx, timer_rx) = cbc::unbounded::<bool>();
    let mut elevator_state = fsm::ElevatorState::init_elevator(elevator_connection.clone(), timer_tx);

    let poll_period = Duration::from_millis(25);

    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();
    {
        let elevator = elevator_connection.clone();
        spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, poll_period));
    }

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
        cbc::select! {
            recv(call_button_rx) -> a => {
                let call_button = a.unwrap();
                elevator_state.fsm_on_request_button_press(call_button.floor as i8, call_button.call);
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
