use driver_rust::elevio::poll::CallButton;
use serde::{Deserialize, Serialize};
use core::time::Duration;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::SystemTime;

use crate::config;
use crate::cost;
use crate::fsm;
use crate::fsm::Dirn;
use crate::fsm::ElevatorBehaviour;
use crate::messages;
use crossbeam_channel as cbc;
use driver_rust::elevio;
use log::{debug, info};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub enum RequestState {
    #[default] None = 0,
    Unconfirmed = 1,
    //Barrier
    Confirmed = 2,
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Request {
    state: RequestState,
    acks: HashSet<u8>
}

impl Request {
    pub fn new(id: u8) -> Request {
        let mut hs = HashSet::new();
        hs.insert(id);
        Request { state: RequestState::None, acks: hs }
    }
    pub fn set_to(&mut self, r: RequestState, id: u8) {
        self.state = r;
        self.acks = HashSet::new();
        self.acks.insert(id);
    }
    pub fn merge(&mut self, r2: &Request, id: u8) -> bool {
        let mut updated: bool = false;
        let new_state = match self.state {
            RequestState::None => {
                match r2.state {
                    RequestState::Unconfirmed => RequestState::Unconfirmed,
                    _ => self.state
                }
            },
            RequestState::Unconfirmed => {
                match r2.state {
                    RequestState::Confirmed => RequestState::Confirmed,
                    _ => self.state
                }
            },
            RequestState::Confirmed => {
                match r2.state {
                    RequestState::None => RequestState::None,
                    _ => self.state
                }
            }
        };
        if self.state != new_state { // state should change
            updated = true;
            self.state = new_state;
            self.acks = r2.acks.clone();
            self.acks.insert(id);
        } else { // state remains the same, but we need to add acks
            self.acks.extend(r2.acks.clone());
        }
        updated
    }

    pub fn get_state(&self) -> RequestState {
        self.state
    }
}
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
            current_floor: -1
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Elevator {
    last_received: SystemTime,
    pub state: ElevatorNetworkState,
    cab_requests: [Request; config::FLOOR_COUNT]
}
impl Elevator {
    pub fn new() -> Elevator {
        let cab_requests = Default::default(); 
        Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState::new(),
            cab_requests
        }
    }

    pub fn get_cab_requests(&self) -> [Request; config::FLOOR_COUNT] {
        self.cab_requests.clone()
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldView {
    id: u8,
    elevators: HashMap<u8, Elevator>,
    hall_requests: [[Request; 2]; config::FLOOR_COUNT]
}

impl WorldView {

    pub fn init(id: u8) -> WorldView {
        let mut elevators = HashMap::new();
        elevators.insert(id, Elevator::new());
        let mut hall_requests: [[Request; 2]; config::FLOOR_COUNT] = Default::default();
        for i in 0..config::FLOOR_COUNT {
            for j in 0..2 {
                hall_requests[i][j] = Request::new(id);
            }
        }
        WorldView {
            id,
            elevators,
            hall_requests
        }
    }
    
    pub fn handle_foreign_world_view(
        &mut self,
        foreign_world_view: WorldView
    ) -> bool {
        let mut updated = false;

        let current_time = SystemTime::now();

        let foreign_id = foreign_world_view.get_id();
        let foreign_elevators= foreign_world_view.get_elevators();

        // add elevators that we dont already know of
        for key in foreign_world_view.elevators.keys() {
            if !self.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
                let u = foreign_world_view.elevators.get(&key).expect("key should have been available");
                self.elevators.insert(*key, u.clone());
            }
        }

        // update foreign elevators state + last_received
        if let Some(e) = foreign_elevators.get(&foreign_id) { 
            let u = self.elevators.get_mut(&foreign_id).expect("key should have been available");
            u.last_received = current_time;
            u.state = e.state;
        }

        // update hall requests
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                let res =  self.hall_requests[floor][dir].merge(&foreign_world_view.hall_requests[floor][dir], self.id);
                if res {
                    info!("Updated HallRequest(floor: {floor}, Type: {dir})");
                }
                updated |= res;
            }
        }

        // update cab requests
        for (id, foreign_elev) in foreign_elevators.iter() {
            let own_elev = self.elevators.get_mut(&id).expect("key should have been available");
            for floor in 0..config::FLOOR_COUNT {
                let res =  own_elev.cab_requests[floor].merge(&foreign_elev.cab_requests[floor], self.id);
                if res {
                    info!("Updated CabRequest(floor: {floor}, Type: 2)");
                }
                updated |= res;
            }
        }

        // update states at barrier
        updated |= self.update_states_at_barrier();
        updated
    }

    pub fn update_states_at_barrier(&mut self) -> bool {
        let mut updated = false;

        // get alive elevators
        let alive_elevators = self.get_alive_elevators(1);

        // go through hall_requests
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                match self.hall_requests[floor][dir].state {
                    RequestState::Unconfirmed => {
                        if alive_elevators.is_subset(&self.hall_requests[floor][dir].acks) {
                            info!("Confirming HallRequest(Floor: {floor}, Type: {dir})");
                            self.hall_requests[floor][dir].set_to(RequestState::Confirmed, self.id);
                            updated = true;
                        }
                    },
                    _ => ()
                }
            }
        }

        // go through cab_requests
        for (_, elev) in self.elevators.iter_mut() {
            for floor in 0..config::FLOOR_COUNT {
                match elev.cab_requests[floor].state {
                    RequestState::Unconfirmed => {
                        if alive_elevators.is_subset(&elev.cab_requests[floor].acks) {
                            info!("Confirming CabRequest(Floor: {floor}, Type: 2)");
                            elev.cab_requests[floor].set_to(RequestState::Confirmed, self.id);
                            updated = true;
                        }
                    },
                    _ => ()
                }
            }
        }
        updated
    }

    pub fn handle_button_press(&mut self, button_press: &CallButton) -> bool {
        let mut updated = false;
        match button_press.call {
            0|1 => { // hall_request?
                match self.hall_requests[button_press.floor as usize][button_press.call as usize].state {
                    RequestState::None => {
                        info!("ButtonPress(Floor: {}, Type: {})", button_press.floor, button_press.call);
                        self.hall_requests[button_press.floor as usize][button_press.call as usize].set_to(RequestState::Unconfirmed, self.id);
                        updated = true;
                    },
                    _ => (),
                }
            },
            2 => { // cab_request?
                let own_elev = self.elevators.get_mut(&self.id).expect("key should have been available");
                match own_elev.cab_requests[button_press.floor as usize].state {
                    RequestState::None => {
                        own_elev.cab_requests[button_press.floor as usize].set_to(RequestState::Unconfirmed, self.id);
                        updated = true;
                    },
                    _ => (),
                }
            },
            _ => ()
    // unknown request
        }
        updated
    }

    pub fn handle_elevator_state(&mut self, dirn: Dirn, behaviour: ElevatorBehaviour, floor: i8) {
        let elev = self.elevators.get_mut(&self.id).expect("key should have been available");
        elev.state.dirn = dirn;
        elev.state.behaviour = behaviour;
        elev.state.current_floor = floor;
    }

    pub fn handle_clear_request(&mut self, floor: usize, should_clear: &[bool; 3]) {
        
        let own_elev = self.elevators.get_mut(&self.id).expect("key should have been available");
        debug!("Clearing {:?}", &should_clear);
        for i in 0..2 {
            if should_clear[i] {
                info!("ClearRequest(Floor: {floor}, Type: {i})");
                self.hall_requests[floor][i].set_to(RequestState::None, self.id);
            }
        }

        if should_clear[2] {
            info!("ClearRequest(Floor: {floor}, Type: 2)");
            own_elev.cab_requests[floor].set_to(RequestState::None, self.id);
        }
    }
    // Getters
    pub fn get_alive_elevators(&self, timeout: u64) -> HashSet<u8> {
        let mut alive_elevators = HashSet::new();
        for (id, elev) in self.elevators.iter() {
            if elev.last_received.elapsed().expect("elapsed() failed") > Duration::from_secs(timeout) && *id != self.id {continue;}
            alive_elevators.insert(*id);
        }
        alive_elevators
    }
    pub fn get_id(&self) -> u8 {
        self.id
    }
    pub fn get_elevators(&self) -> HashMap<u8, Elevator> {
        self.elevators.clone()
    }
    pub fn assign_requests(&self) -> fsm::ControllerRequests {
        let result: fsm::ControllerRequests = cost::elevator_algorythm(&self);
        result
    }
    pub fn get_confirmed_requests(&self) -> [[bool; config::CALL_COUNT]; config::FLOOR_COUNT] {
        let mut requests: [[bool; config::CALL_COUNT]; config::FLOOR_COUNT] = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];
        let elev = self.elevators.get(&self.id).expect("own ID not in elevators");

        
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                match self.hall_requests[floor][dir].state {
                    RequestState::Confirmed => {
                        requests[floor][dir] = true;
                    },
                    _ => (),
                }
            }
            match elev.cab_requests[floor].state {
                RequestState::Confirmed => {
                    requests[floor][2] = true;
                },
                _ => (),
            }
        }

        requests
    }

    pub fn get_hall_requests(&self) -> [[Request; 2]; config::FLOOR_COUNT] {
        self.hall_requests.clone()
    }
}


pub fn run(
    id: u8,
    manager_rx: cbc::Receiver<messages::Manager>,
    sender_tx: cbc::Sender<messages::Manager>,
    controller_tx: cbc::Sender<messages::Controller>,
    lights_tx: cbc::Sender<messages::Controller>,
    call_button_rx: cbc::Receiver<elevio::poll::CallButton>,
    alarm_rx: cbc::Receiver<u8>
) {
    info!("Manager up and running...");
    let mut world_view = WorldView::init(id);
    loop {
        debug!("Current WorldView: {:#?}", &world_view);
        cbc::select! {
            recv(manager_rx) -> a => {
                let message = a.expect("couldn't get message");
                match message {
                    messages::Manager::Ping => {
                        debug!("Received Ping");
                    },
                    messages::Manager::HeartBeat(foreign_world_view) => {
                        debug!("Received WorldView");
                        if foreign_world_view.id != world_view.get_id() {
                            let updated = world_view.handle_foreign_world_view(foreign_world_view);

                            if updated {
                                inform_everybody(
                                    &world_view,
                                    &sender_tx,
                                    &controller_tx,
                                    &lights_tx);
                            }
                        }
                    },
                    messages::Manager::ElevatorState(dirn, behaviour, floor) => {
                        debug!("Received ElevatorState");
                        world_view.handle_elevator_state(dirn, behaviour, floor);
                        
                        inform_everybody(
                            &world_view,
                            &sender_tx,
                            &controller_tx,
                            &lights_tx);
                    },
                    messages::Manager::ClearRequest(floor, should_clear) => {
                        debug!("Received ClearRequest");
                        world_view.handle_clear_request(floor, &should_clear);

                        inform_everybody(
                            &world_view,
                            &sender_tx,
                            &controller_tx,
                            &lights_tx);
                    }
                }
            },
            recv(call_button_rx) -> a => {
                debug!("Received ButtonPress");
                let button_press = a.expect("couldn't get message");
                
                let updated = world_view.handle_button_press(&button_press);
                if updated {
                    inform_everybody(
                        &world_view,
                        &sender_tx,
                        &controller_tx,
                        &lights_tx);
                }

            },
            recv(alarm_rx) -> _a => {
                debug!("Received Alarm");
                world_view.update_states_at_barrier();

                inform_everybody(
                    &world_view,
                    &sender_tx,
                    &controller_tx,
                    &lights_tx);
            }
        }
        debug!("After: {:#?}", &world_view);
    }
}


fn inform_everybody(
    world_view: &WorldView,
    sender_tx: &cbc::Sender<messages::Manager>,
    controller_tx: &cbc::Sender<messages::Controller>,
    lights_tx: &cbc::Sender<messages::Controller>
) {
    let world_view_clone = world_view.clone();
    sender_tx.send(messages::Manager::HeartBeat(world_view_clone)).expect("send to sender failed");
    
    //let controller_reqs = world_view.get_confirmed_requests();
    let controller_reqs = world_view.assign_requests();
    let lights_reqs = world_view.get_confirmed_requests();
    controller_tx.send(messages::Controller::Requests(controller_reqs)).expect("send to controller failed");
    lights_tx.send(messages::Controller::Requests(lights_reqs)).expect("send to lights failed");
}
