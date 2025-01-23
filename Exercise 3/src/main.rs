use std::thread::*;
use std::time::*;

use crossbeam_channel as cbc;

use driver_rust::elevio;
use driver_rust::elevio::elev as e;

mod fsm;

fn main() -> std::io::Result<()> {
    let elev_num_floors = 4;
    let elevator_connection = e::Elevator::init("localhost:15657", elev_num_floors)?;
    let mut elevator_state = fsm::ElevatorState::init_elevator();
    println!("Elevator started:\n{:#?}", elevator_connection);

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
        elevator_connection.motor_direction(fsm::Dirn::Stop as u8);
    }

    loop {
        println!("{:#?}", elevator_state);
        cbc::select! {
            recv(call_button_rx) -> a => {
                let call_button = a.unwrap();
                println!("{:#?}", call_button);
                elevator_state.fsm_on_request_button_press(call_button.floor, call_button.call);
            },
            recv(floor_sensor_rx) -> a => {
                let floor_sensor = a.unwrap();
                println!("{:#?}", floor_sensor);
                elevator_state.fsm_on_floor_arrival(floor_sensor);
            },
            recv(stop_button_rx) -> a => {
                let stop_button = a.unwrap();
                println!("{:#?}", stop_button);
                elevator_state.fsm_on_stop_button_press();
            },
            recv(obstruction_rx) -> a => {
                let obstruction = a.unwrap();
                println!("{:#?}", obstruction);
            },
        }
    }
}
