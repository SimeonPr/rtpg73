use core::time::Duration;
use std::thread::{self, spawn};

use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use log::info;

mod messages;
mod manager;
mod controller;
mod sender;
mod receiver;
mod alarm;
mod fsm;

fn main() {
    env_logger::init();
    info!("Booting application.");
    // create channels
    info!("Creating channels.");
    let (manager_tx, manager_rx) = cbc::unbounded::<messages::Manager>();
    let (controller_tx, controller_rx) = cbc::unbounded::<messages::Controller>();
    let (sender_tx, sender_rx) = cbc::unbounded::<messages::Network>();
    let (alarm_tx, alarm_rx) = cbc::unbounded::<u8>();
    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();

    // create elevator_connection object
    let elev_num_floors = 4;
    let elevator_connection = e::Elevator::init("localhost:15657", elev_num_floors).expect("couldn't create elevator connection");

    info!("Spawning threads.");
    // spawn manager
    let sender_tx_clone = sender_tx.clone();
    let controller_tx_clone = controller_tx.clone();
    let m = spawn(move || manager::run(manager_rx, sender_tx_clone, controller_tx_clone, call_button_rx));
    // spawn controller
    let manager_tx_clone = manager_tx.clone();
    let elev = elevator_connection.clone();
    let c = spawn(move || controller::run(controller_rx, manager_tx_clone, elev));
    // spawn sender
    let s = spawn(move || sender::run(sender_rx));
    // spawn receiver
    let manager_tx_clone = manager_tx.clone();
    let r = spawn(move || receiver::run(manager_tx_clone));
    // spawn call_buttons
    let poll_period = Duration::from_millis(25);
    let elev = elevator_connection.clone();
    let b = spawn(move || elevio::poll::call_buttons(elev, call_button_tx, poll_period));
    
    controller_tx.send(messages::Controller::Ping).unwrap();
    manager_tx.send(messages::Manager::Ping).unwrap();

    let _ = m.join();
    let _ = c.join();
    let _ = s.join();
    let _ = r.join();
    let _ = b.join();
}
