use serde::{Serialize, Deserialize};
use bincode;
use driver_rust::elevio;
#[derive(Debug, Serialize, Deserialize)]
pub enum Manager {
    Ping
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Network {
    Ping
}
#[derive(Debug)]
pub enum Controller {
    Ping,
    Request(elevio::poll::CallButton)
}
