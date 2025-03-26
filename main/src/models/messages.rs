use serde::{Serialize, Deserialize};
use std::time::SystemTime;

use crate::models::{Dirn, ElevatorBehaviour, WorldView, ControllerRequests};

#[derive(Debug, Serialize, Deserialize)]
pub enum Manager {
    Ping(u8),
    Pong(u8),
    NetworkError,
    HeartBeat(SystemTime, WorldView),
    ElevatorState(Dirn, ElevatorBehaviour, i8),
    ClearRequest(usize, [bool; 3]) // floor
}

#[derive(Debug)]
pub enum Controller {
    Requests(ControllerRequests),
}
