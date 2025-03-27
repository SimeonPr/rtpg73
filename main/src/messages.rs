//! Message Types Module
//!
//! Defines the communication protocol between system components.
//! Contains all message types used for inter-module communication.
use serde::{Serialize, Deserialize};
use crate::manager;
use crate::fsm;
#[derive(Debug, Serialize, Deserialize)]

/// Messages sent to and from the Manager module
pub enum Manager {
    HeartBeat(manager::WorldView),
    ElevatorState(fsm::Dirn, fsm::ElevatorBehaviour, i8),
    ClearRequest(usize, [bool; 3]) //floor 
}

/// Messages sent to the Controller module  
#[derive(Debug)]
pub enum Controller {
    Requests(fsm::ControllerRequests)
}
