use super::requests::Request;
use crate::config;
use crate::fsm;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ElevatorNetworkState {
    pub dirn: fsm::Dirn,
    pub behaviour: fsm::ElevatorBehaviour,
    pub current_floor: i8,
}

impl ElevatorNetworkState {
    pub fn new() -> ElevatorNetworkState {
        ElevatorNetworkState {
            dirn: fsm::Dirn::Stop,
            behaviour: fsm::ElevatorBehaviour::Idle,
            current_floor: -1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Elevator {
    last_received: SystemTime,
    pub state: ElevatorNetworkState,
    cab_requests: [Request; config::FLOOR_COUNT],
    last_moved: SystemTime,
    has_request: bool,
    is_working: bool,
}

impl Elevator {
    pub fn new() -> Elevator {
        let cab_requests = Default::default();
        Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState::new(),
            cab_requests,
            last_moved: SystemTime::now(),
            has_request: false,
            is_working: true,
        }
    }

    // Immutable Getters
    pub fn last_received(&self) -> SystemTime {
        self.last_received
    }

    pub fn last_moved(&self) -> SystemTime {
        self.last_moved
    }

    pub fn cab_requests(&self) -> &[Request; config::FLOOR_COUNT] {
        &self.cab_requests
    }

    pub fn cab_requests_mut(&mut self) -> &mut [Request; config::FLOOR_COUNT] {
        &mut self.cab_requests
    }

    pub fn has_request(&self) -> bool {
        self.has_request
    }

    pub fn is_working(&self) -> bool {
        self.is_working
    }

    pub fn get_state(&self) -> ElevatorNetworkState {
        self.state
    }

    // Setters
    pub fn set_last_received(&mut self, time: SystemTime) {
        self.last_received = time;
    }

    pub fn set_last_moved(&mut self, time: SystemTime) {
        self.last_moved = time;
    }

    pub fn set_has_request(&mut self, value: bool) {
        self.has_request = value;
    }

    pub fn set_is_working(&mut self, value: bool) {
        self.is_working = value;
    }

    pub fn get_cab_requests(&self) -> &[Request; config::FLOOR_COUNT] {
        &self.cab_requests
    }
}
