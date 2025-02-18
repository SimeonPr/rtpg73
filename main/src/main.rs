use core::time::Duration;
use std::thread::spawn;

use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;

mod messages;
mod manager;
mod controller;
mod sender;
mod receiver;
mod alarm;

fn main() {
    
    // create channels
    let (manager_tx, manager_rx) = cbc::unbounded::<u8>();
    let (controller_tx, controller_rx) = cbc::unbounded::<u8>();
    let (sender_tx, sender_rx) = cbc::unbounded::<u8>();
    let (alarm_tx, alarm_rx) = cbc::unbounded::<u8>();
    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();

    // create elevator_connection object
    let elev_num_floors = 4;
    let elevator_connection = e::Elevator::init("localhost:15657", elev_num_floors).expect("couldn't create elevator connection");
    
    // spawn manager
    let m = spawn(move || manager::run());
    // spawn controller
    let c = spawn(move || controller::run());
    // spawn sender
    let s = spawn(move || sender::run());
    // spawn receiver
    let r = spawn(move || receiver::run());
    // spawn call_buttons
    let poll_period = Duration::from_millis(25);
    let b = spawn(move || elevio::poll::call_buttons(elevator_connection, call_button_tx, poll_period));

    let _ = m.join();
    let _ = c.join();
    let _ = s.join();
    let _ = r.join();
    let _ = b.join();
}

