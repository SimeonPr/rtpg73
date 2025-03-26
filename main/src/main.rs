use std::{
    env,
    panic,
    process,
    thread::spawn,
    time::Duration,
};

use crossbeam_channel as cbc;
use driver_rust::elevio::{self, elev as e};
use log::info;

// Internal modules
mod alarm;
mod config;
mod controller;
mod cost;
mod lights;
mod manager;
mod receiver;
mod sender;
pub mod models;

// Models-specific imports
use crate::models::{fsm, messages};

fn main() {
    // crash on any thread panic
    panic::set_hook(Box::new(|info| {
        eprintln!("A thread panicked: {:?}. \nExiting...", info);
        process::abort();
    }));

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

    let id = id.unwrap_or(0);
    info!("Running with ID {}", id);

    env_logger::init();
    info!("Booting application.");
    info!("Creating channels.");

    let (manager_tx, manager_rx) = cbc::unbounded::<messages::Manager>();
    let (controller_tx, controller_rx) = cbc::unbounded::<messages::Controller>();
    let (lights_tx, lights_rx) = cbc::unbounded::<messages::Controller>();
    let (sender_tx, sender_rx) = cbc::unbounded::<messages::Manager>();
    let (alarm_tx, alarm_rx) = cbc::unbounded::<u8>();
    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();

    let elev_num_floors = 4;
    let address = env::var("ELEVATOR_PORT")
        .map(|port| format!("host.docker.internal:{}", port))
        .unwrap_or_else(|_| "127.0.0.1:15657".into());

    let elevator_connection =
        e::Elevator::init(&address, elev_num_floors).expect("hardware must be available");

    info!("Spawning threads.");

    let m = spawn({
        let sender_tx = sender_tx.clone();
        let controller_tx = controller_tx.clone();
        let lights_tx = lights_tx.clone();
        let alarm_rx = alarm_rx.clone();
        move || {
            manager::run(
                id,
                manager_rx,
                sender_tx,
                controller_tx,
                lights_tx,
                call_button_rx,
                alarm_rx,
            )
        }
    });

    let l = spawn({
        let lights_rx = lights_rx.clone();
        let elev = elevator_connection.clone();
        move || lights::run(lights_rx, elev)
    });

    let c = spawn({
        let manager_tx = manager_tx.clone();
        let elev = elevator_connection.clone();
        move || controller::run(controller_rx, manager_tx, elev)
    });

    let s = spawn({
        let manager_tx = manager_tx.clone();
        move || sender::run(sender_rx, manager_tx)
    });

    let r = spawn({
        let manager_tx = manager_tx.clone();
        move || receiver::run(manager_tx)
    });

    let b = spawn({
        let poll_period = Duration::from_millis(25);
        let elev = elevator_connection.clone();
        move || elevio::poll::call_buttons(elev, call_button_tx, poll_period)
    });

    let a = spawn({
        let timeout = Duration::from_secs(1);
        let alarm_tx = alarm_tx.clone();
        move || alarm::run(alarm_tx, timeout)
    });

    let _ = m.join();
    let _ = l.join();
    let _ = c.join();
    let _ = s.join();
    let _ = r.join();
    let _ = b.join();
    let _ = a.join();
}
