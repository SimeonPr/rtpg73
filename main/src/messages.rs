use serde::{Serialize, Deserialize};
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
    Requests(fsm::ControllerRequests)
}
