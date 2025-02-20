use serde::{Deserialize, Serialize};
use std::arch::x86_64::_MM_FROUND_FLOOR;
use std::collections::HashMap;
use std::time::SystemTime;

use crate::config;
use crate::config::FLOOR_COUNT;
use crate::fsm;
use crate::messages;
use crossbeam_channel as cbc;
use driver_rust::elevio;
use log::{debug, info};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum RequestState {
    None = 0,
    Unconfirmed = 1,
    Confirmed = 2,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ElevatorNetworkState {
    cab_requests: [bool; config::FLOOR_COUNT],
    dirn: fsm::Dirn,
    behavior: fsm::ElevatorBehaviour,
    current_floor: i8,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Elevator {
    id: u8, // any nodes id
    last_received: SystemTime,
    available: bool, // remove 
    state: ElevatorNetworkState,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldView {
    id: u8, // our id
    elevators: HashMap<u8, Elevator>,
    hall_requests: HashMap<u8, [[RequestState; 2]; config::FLOOR_COUNT]>,
}

impl WorldView {
    pub fn init(id: u8) -> WorldView {
        let mut elevators = HashMap::new();
        let our_elevator = Elevator {
            id,
            last_received: SystemTime::now(),
            available: true,
            state: ElevatorNetworkState {
                cab_requests: [false; config::FLOOR_COUNT],
                dirn: fsm::Dirn::Stop,
                behavior: fsm::ElevatorBehaviour::Idle,
                current_floor: -1,
            },
        };
        elevators.insert(id, our_elevator);

        WorldView {
            id,
            elevators,
            hall_requests: HashMap::new(),
        }
    }

    pub fn update(
        &mut self,
        id: u8,
        net_state: ElevatorNetworkState,
        hall_reqs: HashMap<u8, [[RequestState; 2]; config::FLOOR_COUNT]>,
    ) {
        let current_time = SystemTime::now();

        let new_elev = Elevator {
            id,
            last_received: current_time,
            available: true,
            state: net_state,
        };
        self.elevators.insert(id, new_elev);
        let u = hall_reqs.get(&id).unwrap();
        self.hall_requests.insert(id, u.clone());
        for key in hall_reqs.keys() {
            if !self.hall_requests.contains_key(key) {
                let u = hall_reqs.get(key).unwrap();
                self.hall_requests.insert(*key, u.clone());
            }
        }
        // keys in hall_reqs are subset of keys in self.hall_requests
        for (key, value) in hall_reqs.iter() {
            let our_value = self.hall_requests.get(key).unwrap();
            let mut new_value: [[RequestState; 2]; FLOOR_COUNT] =
                [[RequestState::None; 2]; FLOOR_COUNT];
            for floor in 0..config::FLOOR_COUNT {
                for dir in 0..2 {
                    new_value[floor][dir] = match value[floor][dir] {
                        RequestState::None => match our_value[floor][dir] {
                            RequestState::None => RequestState::None,
                            RequestState::Unconfirmed => RequestState::Unconfirmed,
                            RequestState::Confirmed => RequestState::None,
                        },
                        RequestState::Unconfirmed => match our_value[floor][dir] {
                            RequestState::None => RequestState::Unconfirmed,
                            RequestState::Unconfirmed => RequestState::Unconfirmed,
                            RequestState::Confirmed => RequestState::Confirmed,
                        },
                        RequestState::Confirmed => match our_value[floor][dir] {
                            RequestState::None => RequestState::None,
                            RequestState::Unconfirmed => RequestState::Confirmed,
                            RequestState::Confirmed => RequestState::Confirmed,
                        },
                    };
                }
            }

            self.hall_requests.insert(*key, new_value);
        }

        let mut alive_elevators: Vec<u8> = Vec::new();
        for (key, value) in self.elevators.iter() {
            if value.available {
                alive_elevators.push(*key);
            }
        }

        for floor in 0..FLOOR_COUNT {
            for dir in 0..2 {
                let mut counter = 0; 
                for id in alive_elevators.iter() {
                    let e = self.hall_requests.get(id).unwrap();
                    if e[floor][dir] == RequestState::Unconfirmed{
                        couter +=1
                    }
                }
                if counter > alive_elevators.len(){
                    
                }
            }
        }
    }
}

pub fn run(
    manager_rx: cbc::Receiver<messages::Manager>,
    sender_tx: cbc::Sender<messages::Network>,
    controller_tx: cbc::Sender<messages::Controller>,
    call_button_rx: cbc::Receiver<elevio::poll::CallButton>,
) {
    debug!("Manager up and running...");
    let mut world_view = WorldView::init(1);
    let mut fresh: bool = true;
    loop {
        debug!("Waiting for input...");
        cbc::select! {
            recv(manager_rx) -> a => {
                let message = a.unwrap();
                match message {
                    messages::Manager::Ping => {
                        info!("Received ping");
                    },
                    messages::Manager::HeartBeat(id, net_state, hr) => {
                        info!("Received HeartBeat");
                        world_view.update(id, net_state, hr);
                        let world_view_clone = world_view.clone();
                        sender_tx.send(messages::Network::HeartBeat(world_view_clone)).unwrap();
                    }
                }
            },
            recv(call_button_rx) -> a => {
                let message = a.unwrap();
                info!("Received button press");
                debug!("{:?}", message);
                // forward to controller
                controller_tx.send(messages::Controller::Request(message)).unwrap();
            }
        }
    }
}
