use core::time::Duration;
use std::thread::{self, spawn};

use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use log::info;
use manager::{RequestState, WorldView};

mod messages;
mod manager;
mod controller;
mod sender;
mod receiver;
mod alarm;
mod fsm;
mod config;
use std::env;

fn main() {

    let args: Vec<String> = env::args().collect();
    
    let mut id: Option<u8> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--id" {
            if let Some(value) = iter.next() {
                id = value.parse().ok();
            }
        }
    }

    match id {
        Some(id) => println!("ID: {}", id),
        _ => {
            println!("Using default id 0");
            id = Some(0);
        },
    }

    env_logger::init();
    info!("Booting application.");
    // create channels
    info!("Creating channels.");
    let (manager_tx, manager_rx) = cbc::unbounded::<messages::Manager>();
    let (controller_tx, controller_rx) = cbc::unbounded::<messages::Controller>();
    let (sender_tx, sender_rx) = cbc::unbounded::<messages::Manager>();
    let (alarm_tx, alarm_rx) = cbc::unbounded::<u8>();
    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();

    // create elevator_connection object
    let elev_num_floors = 4;
    let elevator_connection = e::Elevator::init("localhost:15657", elev_num_floors).expect("couldn't create elevator connection");

    info!("Spawning threads.");
    // spawn manager
    let sender_tx_clone = sender_tx.clone();
    let controller_tx_clone = controller_tx.clone();
    let alarm_rx_clone = alarm_rx.clone();
    let m = spawn(move || manager::run(id.unwrap(), manager_rx, sender_tx_clone, controller_tx_clone, call_button_rx, alarm_rx_clone));
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
    // spawn alarm
    let timeout = Duration::from_secs(10);
    let alarm_tx_clone = alarm_tx.clone();
    let a = spawn(move || alarm::run(alarm_tx_clone, timeout));


    // Test Block
    let mut init_requests = [[manager::RequestState::None;3]; config::FLOOR_COUNT];
    init_requests[0][2] = RequestState::Unconfirmed;
    let wv = WorldView::init_with_requests(5, init_requests);
    manager_tx.send(messages::Manager::HeartBeat(wv)).unwrap();

    let mut init_requests = [[manager::RequestState::None;3]; config::FLOOR_COUNT];
    init_requests[0][2] = RequestState::Confirmed;
    let wv = WorldView::init_with_requests(5, init_requests);
    manager_tx.send(messages::Manager::HeartBeat(wv)).unwrap();


    
    let _ = m.join();
    let _ = c.join();
    let _ = s.join();
    let _ = r.join();
    let _ = b.join();
    let _ = a.join();
}
