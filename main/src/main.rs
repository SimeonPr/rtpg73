use core::time::Duration;
use std::thread::spawn;
use std::{process, panic};
use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use log::info;

mod messages;
mod manager;
mod controller;
mod sender;
mod receiver;
mod cost;
mod alarm;
mod lights;
mod fsm;
mod config;
use std::env;

fn get_id_from_args(args: Vec<String>) -> u8 {
    let mut id: Option<u8> = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        if arg == "--id" {
            if let Some(value) = iter.next() {
                id = value.parse().ok();
            }
        }
    }

    id.unwrap_or(0)
}

fn main() {

    // crash on any thread panic
    panic::set_hook(Box::new(|info| {
        eprintln!("A thread panicked: {:?}. \nExiting...", info);
        process::abort();
    }));

    let args: Vec<String> = env::args().collect();
    
    let id: u8 = get_id_from_args(args);

    info!("Running with ID {}", id);
    env_logger::init();
    info!("Booting application.");
    // create channels
    info!("Creating channels.");
    let (manager_tx, manager_rx) = cbc::unbounded::<messages::Manager>();
    let (controller_tx, controller_rx) = cbc::unbounded::<messages::Controller>();
    let (lights_tx, lights_rx) = cbc::unbounded::<messages::Controller>();
    let (sender_tx, sender_rx) = cbc::unbounded::<messages::Manager>();
    let (alarm_tx, alarm_rx) = cbc::unbounded::<u8>();
    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();

    // create elevator_connection object
    let elev_num_floors = 4;
    let address = match env::var("ELEVATOR_PORT") {
        Ok(port) => format!("host.docker.internal:{}", port),
        Err(_) => format!("127.0.0.1:15657")
    };

    let elevator_connection =
        e::Elevator::init(&address, elev_num_floors).expect("hardware must be available");

    info!("Spawning threads.");
    // spawn manager
    let sender_tx_clone = sender_tx.clone();
    let controller_tx_clone = controller_tx.clone();
    let alarm_rx_clone = alarm_rx.clone();
    let lights_tx_clone = lights_tx.clone();
    let m = spawn(move || manager::run(
        id,
        manager_rx,
        sender_tx_clone,
        controller_tx_clone,
        lights_tx_clone,
        call_button_rx,
        alarm_rx_clone));
    // spawn lights handler
    let lights_rx_clone = lights_rx.clone();
    let elev = elevator_connection.clone();
    let l = spawn(move || lights::run(lights_rx_clone, elev));
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
    let timeout = Duration::from_secs(1);
    let alarm_tx_clone = alarm_tx.clone();
    let a = spawn(move || alarm::run(alarm_tx_clone, timeout));

    let _ = m.join();
    let _ = l.join();
    let _ = c.join();
    let _ = s.join();
    let _ = r.join();
    let _ = b.join();
    let _ = a.join();
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_id_from_args_test() {
        let args = vec!["--id".to_string(), "1".to_string()];
        assert_eq!(get_id_from_args(args), 1);
    }

    #[test]
    fn get_id_from_args_test_panic() {
        let args = vec!["--id".to_string()];
        assert_eq!(get_id_from_args(args), 0);
    }

    #[test]
    fn get_id_from_args_test_panic2() {
        let args = vec!["--id".to_string(), "a".to_string()];
        assert_eq!(get_id_from_args(args), 0);
    }
}
