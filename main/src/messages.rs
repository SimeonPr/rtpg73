use std::collections::HashMap;

use serde::{Serialize, Deserialize};
use bincode;
use driver_rust::elevio;
use crate::manager;
use crate::config;
#[derive(Debug, Serialize, Deserialize)]
pub enum Manager {
    Ping,
    HeartBeat(u8, manager::ElevatorNetworkState, HashMap<u8, [[manager::RequestState; 2]; config::FLOOR_COUNT]>)
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Network {
    Ping,
    HeartBeat(manager::WorldView)
}
#[derive(Debug)]
pub enum Controller {
    Ping,
    Request(elevio::poll::CallButton)
}
