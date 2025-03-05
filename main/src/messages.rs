use serde::{Serialize, Deserialize};
use driver_rust::elevio;
use crate::manager;
use crate::fsm;
#[derive(Debug, Serialize, Deserialize)]
pub enum Manager {
    Ping,
    HeartBeat(manager::WorldView),
    ElevatorState(fsm::Dirn, fsm::ElevatorBehaviour, i8),
    ClearRequest(usize, [bool; 3]) //floor 
}

#[derive(Debug)]
pub enum Controller {
    Ping,
    Requests(fsm::ControllerRequests)
}
