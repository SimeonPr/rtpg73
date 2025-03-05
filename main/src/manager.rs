use driver_rust::elevio::poll::CallButton;
use serde::{Deserialize, Serialize};
use core::time::Duration;
use std::collections::HashMap;
use std::time::SystemTime;

use crate::config;
use crate::fsm;
use crate::fsm::ControllerRequests;
use crate::fsm::Dirn;
use crate::fsm::ElevatorBehaviour;
use crate::messages;
use crossbeam_channel as cbc;
use driver_rust::elevio;
use log::{debug, info};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum RequestState {
    None = 0,
    Unconfirmed = 1,
    //Barrier
    Confirmed = 2,
}
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ElevatorNetworkState {
    dirn: fsm::Dirn,
    behaviour: fsm::ElevatorBehaviour,
    current_floor: i8,
}
impl ElevatorNetworkState {
    pub fn get_dirn(&self) -> fsm::Dirn {
        self.dirn
    }
    pub fn get_behaviour(&self) -> fsm::ElevatorBehaviour {
        self.behaviour
    }
    pub fn get_current_floor(&self) -> i8 {
        self.current_floor
    }
}
pub type ManagerRequests = [[RequestState; 3]; config::FLOOR_COUNT];
pub fn manager_to_controller_requests(manager_reqs: &ManagerRequests) -> ControllerRequests {
    let mut controller_requests: ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];
    for floor in 0..config::FLOOR_COUNT {
        for call in 0..config::CALL_COUNT {
            controller_requests[floor][call] = match manager_reqs[floor][call] {
                RequestState::Confirmed => true,
                _ => false
            };
        }
    }
    controller_requests
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Elevator {
    last_received: SystemTime,
    state: ElevatorNetworkState,
    requests: ManagerRequests
}
impl Elevator {
    pub fn get_last_received(&self) -> SystemTime {
        self.last_received
    }
    pub fn get_state(&self) -> ElevatorNetworkState {
        self.state
    }
    pub fn get_requests(&self) -> ManagerRequests {
        self.requests
    }
    pub fn set_last_received(&mut self, new_val: SystemTime) {
        self.last_received = new_val;
    }
    pub fn set_state(&mut self, new_val: ElevatorNetworkState) {
        self.state = new_val;
    }
    pub fn set_requests(&mut self, new: ManagerRequests) {
        self.requests = new;
    }

}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldView {
    id: u8,
    pub elevators: HashMap<u8, Elevator>,
}

impl WorldView {
    pub fn init_with_requests(id: u8, init_requests: ManagerRequests) -> WorldView {
        let mut elevators = HashMap::new();
        let our_elevator = Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState {
                dirn: fsm::Dirn::Stop,
                behaviour: fsm::ElevatorBehaviour::Idle,
                current_floor: -1,
            },
            requests: init_requests
        };
        elevators.insert(id, our_elevator);
        WorldView {
            id,
            elevators
        }
    }
    pub fn init(id: u8) -> WorldView {
        let mut elevators = HashMap::new();
        let requests = [[RequestState::None; 3]; config::FLOOR_COUNT];
        let our_elevator = Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState {
                dirn: fsm::Dirn::Stop,
                behaviour: fsm::ElevatorBehaviour::Idle,
                current_floor: -1,
            },
            requests
        };
        elevators.insert(id, our_elevator);
        WorldView {
            id,
            elevators
        }
    }

    pub fn handle_foreign_world_view(
        &mut self,
        foreign_world_view: WorldView,
        controller_tx: &cbc::Sender<messages::Controller>,
        sender_tx: &cbc::Sender<messages::Manager>
    ) {
        if foreign_world_view.id == self.id {return;}

        let current_time = SystemTime::now();

        let foreign_id = foreign_world_view.get_id();
        let foreign_elevators= foreign_world_view.get_elevators();

        // update local elevator for id
        if let Some(e) = foreign_elevators.get(&foreign_id) { // get foreign elevator
            // update local version
            self.elevators.insert(foreign_id, e.clone());
            let local_elevator = self.elevators.get_mut(&foreign_id).unwrap();
            local_elevator.set_last_received(current_time);
        }


        // add elevators that we dont already know of
        for key in foreign_world_view.elevators.keys() {
            if !self.elevators.contains_key(key) {
                let u = foreign_world_view.elevators.get(&key).unwrap();
                self.elevators.insert(*key, u.clone());
            }
        }

        // for each id, floor, direction update the counter based on our value and received value
        for (id, their_elevator) in foreign_world_view.elevators.iter() {
            if *id == self.id {continue;}
            let our_elevator = self.elevators.get_mut(id).unwrap();
            for floor in 0..config::FLOOR_COUNT {
                for dir in 0..3 {
                    our_elevator.requests[floor][dir] = match their_elevator.requests[floor][dir] {
                        RequestState::None => match our_elevator.requests[floor][dir] {
                            RequestState::None => RequestState::None,
                            RequestState::Unconfirmed => RequestState::Unconfirmed,
                            RequestState::Confirmed => RequestState::None,
                        },
                        RequestState::Unconfirmed => match our_elevator.requests[floor][dir] {
                            RequestState::None => RequestState::Unconfirmed,
                            RequestState::Unconfirmed => RequestState::Unconfirmed,
                            RequestState::Confirmed => RequestState::Confirmed,
                        },
                        RequestState::Confirmed => match our_elevator.requests[floor][dir] {
                            RequestState::None => RequestState::None,
                            RequestState::Unconfirmed => RequestState::Confirmed,
                            RequestState::Confirmed => RequestState::Confirmed,
                        },
                    };
                }
            }
        }

        // somehow merge the different states of responsive elevators into a new state
        let mut new_requests: ManagerRequests = [[RequestState::None; 3]; config::FLOOR_COUNT];
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..3 {

                // store request state for floor/direction in tmp_vector
                let mut tmp_vector: Vec<RequestState> = Vec::new();
                for (id, elevator) in self.elevators.iter() {
                    // only include alive elevators (and ourselves)
                    if elevator.last_received.elapsed().unwrap() > Duration::from_secs(1) && *id != self.id {
                        continue;
                    }
                    
                    tmp_vector.push(elevator.requests[floor][dir]);
                }

                let mut count = [0;3]; // counts the occurences of a state for floor/dir
                for val in tmp_vector.iter() {
                    match val {
                        RequestState::None => {
                            count[0] += 1;
                        },
                        RequestState::Unconfirmed => {
                            count[1] += 1;
                        },
                        RequestState::Confirmed => {
                            count[2] += 1;
                        }
                    }
                }

                // all at barrier
                if count[0] == 0 && count[2] == 0 { // [0 n 0]
                    new_requests[floor][dir] = RequestState::Confirmed;
                } else {
                    match self.elevators.get(&self.id).unwrap().requests[floor][dir] {
                        RequestState::None => {
                            if count[1] > 0 {
                                new_requests[floor][dir] = RequestState::Unconfirmed;
                            }
                        },
                        RequestState::Unconfirmed => {
                            if count[2] > 0 {
                                new_requests[floor][dir] = RequestState::Confirmed;
                            }
                        },
                        RequestState::Confirmed => {
                            if count[0] > 0 {
                                new_requests[floor][dir] = RequestState::None;
                            }
                        }
                    }
                }
            }
        }


        // replace old hall requests with new hall requests
        self.elevators.get_mut(&self.id).unwrap().requests = new_requests;

        // notify relevant subsystems
        let world_view_clone = self.clone();
        sender_tx.send(messages::Manager::HeartBeat(world_view_clone)).unwrap();
        let manager_reqs: ManagerRequests = self.elevators.get(&self.id).unwrap().requests;
        let controller_reqs = manager_to_controller_requests(&manager_reqs);
        controller_tx.send(messages::Controller::Requests(controller_reqs)).unwrap();
    }
    pub fn handle_button_press(&mut self, button_press: &CallButton) {
        let new_value = match self.elevators.get(&self.id).unwrap().requests[button_press.floor as usize][button_press.call as usize] {
            RequestState::None => RequestState::Unconfirmed,
            RequestState::Unconfirmed => RequestState::Unconfirmed,
            RequestState::Confirmed => RequestState::Confirmed
        };
        self.elevators.get_mut(&self.id).unwrap().requests[button_press.floor as usize][button_press.call as usize] = new_value;

        //notify relevant subsystems
    }

    pub fn handle_elevator_state(&mut self, dirn: Dirn, behaviour: ElevatorBehaviour, floor: i8, sender_tx: &cbc::Sender<messages::Manager>) {
        let mut state = self.elevators.get_mut(&self.id).unwrap().state;
        state.dirn = dirn;
        state.behaviour = behaviour;
        state.current_floor = floor;

        let world_view_clone = self.clone();
        sender_tx.send(messages::Manager::HeartBeat(world_view_clone)).unwrap();
    }
    // Getters
    pub fn get_id(&self) -> u8 {
        self.id
    }
    pub fn get_elevators(&self) -> HashMap<u8, Elevator> {
        self.elevators.clone()
    }
}

pub fn run(
    id: u8,
    manager_rx: cbc::Receiver<messages::Manager>,
    sender_tx: cbc::Sender<messages::Manager>,
    controller_tx: cbc::Sender<messages::Controller>,
    call_button_rx: cbc::Receiver<elevio::poll::CallButton>,
    alarm_rx: cbc::Receiver<u8>,
) {
    debug!("Manager up and running...");
    let mut world_view = WorldView::init(id);
    loop {
        debug!("Waiting for input...");
        cbc::select! {
            recv(manager_rx) -> a => {
                let message = a.unwrap();
                match message {
                    messages::Manager::Ping => {
                        info!("Received Ping");
                    },
                    messages::Manager::HeartBeat(foreign_world_view) => {
                        info!("Received HeartBeat from {}", foreign_world_view.get_id());
                        debug!("Before: {:#?}", &world_view);
                        world_view.handle_foreign_world_view(foreign_world_view, &controller_tx, &sender_tx);
                        debug!("After: {:#?}", &world_view);
                    },
                    messages::Manager::ElevatorState(dirn, behaviour, floor) => {
                        world_view.handle_elevator_state(dirn, behaviour, floor, &sender_tx);
                    }
                }
            },
            recv(call_button_rx) -> a => {
                let button_press = a.unwrap();
                info!("Received Button Press");
                debug!("{:?}", button_press);
                debug!("Before: {:#?}", world_view.clone());
                world_view.handle_button_press(&button_press);
                debug!("After: {:#?}", world_view.clone());
                let manager_reqs: ManagerRequests = world_view.get_elevators().get(&world_view.get_id()).unwrap().requests;
                let controller_reqs = manager_to_controller_requests(&manager_reqs);
                controller_tx.send(messages::Controller::Requests(controller_reqs)).unwrap();
            },
            recv(alarm_rx) -> _a => {
                info!("Received Alarm");
                let world_view_clone = world_view.clone();
                sender_tx.send(messages::Manager::HeartBeat(world_view_clone)).unwrap();
            }
        }
    }
}
