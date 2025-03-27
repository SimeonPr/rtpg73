//! Elevator Management System
//!
//! This module implements the core logic for managing multiple elevators in a distributed system.
//! It handles:
//! - Request state tracking and synchronization
//! - Elevator state management
//! - Distributed consensus for request handling
//! - Failure detection and recovery
use driver_rust::elevio::poll::CallButton;
use log::error;
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

/// Represents the state of an elevator request
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub enum RequestState {
    #[default] None = 0,
    Unconfirmed = 1,
    //Barrier
    Confirmed = 2,
    Finished = 3,
}

/// Represents an individual elevator request with state and acknowledgments
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Request {
    state: RequestState,
    acks: HashSet<u8>
}

impl Request {
    /// Creates a new request with default state and initial acknowledgment
    pub fn new(id: u8) -> Request {
        let mut hs = HashSet::new();
        hs.insert(id);
        Request { state: RequestState::None, acks: hs }
    }

    /// Updates the request state and resets acknowledgments
    pub fn set_to(&mut self, r: RequestState, id: u8) {
        self.state = r;
        self.acks = HashSet::new();
        self.acks.insert(id);
    }

    /// Merges this request with another request's state
    /// Returns true if the state changed as a result
    pub fn merge(&mut self, r2: &Request, id: u8) -> bool {
        let mut updated: bool = false;
        let new_state = match (self.state, r2.state) {
            (RequestState::None, RequestState::Unconfirmed) => RequestState::Unconfirmed,
            (RequestState::None, RequestState::Confirmed) => RequestState::Confirmed,
            (RequestState::Unconfirmed, RequestState::Confirmed) => RequestState::Confirmed,
            (RequestState::Confirmed, RequestState::Finished) => RequestState::Finished,
            (RequestState::Finished, RequestState::None) => RequestState::None,
            (RequestState::Finished, RequestState::Unconfirmed) => RequestState::Unconfirmed,
            _ => self.state
        };
        if self.state != new_state { // state changed
            updated = true;
            self.state = new_state;
            self.acks = r2.acks.clone();
            self.acks.insert(id);
        } else if self.state == r2.state { // state remains the same, but we need to add acks
            self.acks.extend(r2.acks.clone());
        }
        updated
    }

    /// Returns the current request state
    pub fn get_state(&self) -> RequestState {
        self.state
    }
}

/// Network representation of an elevator's state
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

/// Represents an elevator in the system
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Elevator {
    last_received: SystemTime,
    pub state: ElevatorNetworkState,
    cab_requests: [Request; config::FLOOR_COUNT],
    last_moved: SystemTime,
    has_request: bool,
    pub is_working: bool
}
impl Elevator {
    /// Creates a new elevator instance
    pub fn new() -> Elevator {
        let cab_requests = Default::default(); 
        Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState::new(),
            cab_requests,
            last_moved: SystemTime::now(),
            has_request: false,
            is_working: true

        }
    }

    /// Returns a copy of all cab requests
    pub fn get_cab_requests(&self) -> [Request; config::FLOOR_COUNT] {
        self.cab_requests.clone()
    }
}

/// Complete system view containing all elevators and requests
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldView {
    pub(crate) id: u8,
    pub(crate) elevators: HashMap<u8, Elevator>,
    pub(crate) hall_requests: [[Request; 2]; config::FLOOR_COUNT]
}

impl WorldView {
    /// Initializes a new WorldView for an elevator
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

    /// Compares this WorldView with another and logs differences
    pub fn compare_world_views(
        &self,
        other_world_view: &WorldView
    ) {
        // Hall requests
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if self.hall_requests[floor][dir].state != other_world_view.hall_requests[floor][dir].state {
                    info!(
                        "HallRequestState(floor: {}, dir: {}): {:?} -> {:?}",
                        floor,
                        dir,
                        self.hall_requests[floor][dir].state,
                        other_world_view.hall_requests[floor][dir].state
                    );
                }
            }
        }
        // new elevators?
        for key in other_world_view.elevators.keys() {
            if !self.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
            }
        }
        // Cab Requests + network state
        for (key, elev) in self.elevators.iter() {
            if other_world_view.elevators.contains_key(key) {
                let other_elev = other_world_view.elevators.get(key).expect("we just checked that");
                for floor in 0..config::FLOOR_COUNT {
                    if elev.cab_requests[floor].state != other_elev.cab_requests[floor].state {
                        info!(
                            "CabRequestState(id: {}, floor: {}, dir: {}): {:?} -> {:?}",
                            key,
                            floor,
                            2,
                            elev.cab_requests[floor].state,
                            other_elev.cab_requests[floor].state
                        );
                    }
                    
                }
                // dirn
                if elev.state.dirn != other_elev.state.dirn {
                    info!(
                        "Dirn(id: {}): {:?} -> {:?}",
                        key,
                        elev.state.dirn,
                        other_elev.state.dirn
                    );
                }
                if elev.state.current_floor != other_elev.state.current_floor {
                    info!(
                        "CurrentFloor(id: {}): {} -> {}",
                        key,
                        elev.state.current_floor,
                        other_elev.state.current_floor
                    );
                }
                if elev.state.behaviour != other_elev.state.behaviour {
                    info!(
                        "Behaviour(id: {}): {:?} -> {:?}",
                        key,
                        elev.state.behaviour,
                        other_elev.state.behaviour
                    );
                }
            } else {
                info!("ID {} removed", key);
            }
        }
    }

    /// Handles recovery by adopting a foreign WorldView humbly
    pub fn handle_humbly(
        &self,
        foreign_world_view: WorldView
    ) -> WorldView {
        let mut wv_clone = self.clone();
        info!("Humble Recovery");
        let id = wv_clone.id;
        let old_elevator = wv_clone.elevators.get(&id).unwrap().clone();
        wv_clone = foreign_world_view.clone();
        wv_clone.id = id;
        if !wv_clone.elevators.contains_key(&id) {
            wv_clone.elevators.insert(id, old_elevator.clone());
        }
        wv_clone
    }
    pub fn handle_network_recovery(
        &self,
        foreign_world_view: WorldView
    ) -> WorldView {
        let mut wv_clone = self.clone();
        info!("Network Recovery");
        wv_clone.hall_requests = foreign_world_view.hall_requests;
        for (key, elev) in foreign_world_view.elevators {
            if key == self.id {continue;}
            wv_clone.elevators.insert(key, elev);
        }
        wv_clone
    }

    /// Merges a foreign WorldView into this one
    /// Returns updated WorldView and whether changes were made
    pub fn handle_foreign_world_view(
        &self,
        foreign_world_view: WorldView
    ) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let mut updated = false;

        let current_time = SystemTime::now();

        let foreign_id = foreign_world_view.get_id();
        let foreign_elevators= foreign_world_view.get_elevators();

        // add elevators that we dont already know of
        for (key, elev) in foreign_world_view.elevators.iter() {
            if !wv_clone.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
                wv_clone.elevators.insert(*key, elev.clone());
            }
        }

        // update foreign elevators state + last_received
        if let Some(e) = foreign_elevators.get(&foreign_id) { 
            let u = wv_clone.elevators.get_mut(&foreign_id).expect("key should have been available");
            if (u.state.current_floor != e.state.current_floor) || (u.state.behaviour != e.state.behaviour){
                if !u.is_working {
                    info!("Foreign Elevator {} recovered", foreign_id);
                }
                u.last_moved = SystemTime::now();
                u.is_working = true;
            }
            u.last_received = current_time;
            u.state = e.state;
        }

        // update hall requests
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                let res = wv_clone.hall_requests[floor][dir].merge(&foreign_world_view.hall_requests[floor][dir], wv_clone.id);
                updated |= res;
            }
        }

        // update cab requests
        for (id, foreign_elev) in foreign_elevators.iter() {
            let own_elev = wv_clone.elevators.get_mut(&id).expect("key should have been available");
            for floor in 0..config::FLOOR_COUNT {
                let res = own_elev.cab_requests[floor].merge(&foreign_elev.cab_requests[floor], wv_clone.id);
                updated |= res;
            }
        }

        // update states at barrier
        let (wv_clone, tmp_updated) = wv_clone.update_states_at_barrier();
        updated |= tmp_updated;
        (wv_clone, updated)
    }

    /// Updates request states that have reached consensus
    /// Returns updated WorldView and whether changes were made
    pub fn update_states_at_barrier(&self) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let mut updated = false;

        // get alive elevators
        let alive_elevators = wv_clone.get_alive_elevators(2);

        // go through hall_requests
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                match wv_clone.hall_requests[floor][dir].state {
                    RequestState::Unconfirmed => {
                        if alive_elevators.is_subset(&wv_clone.hall_requests[floor][dir].acks) {
                            wv_clone.hall_requests[floor][dir].set_to(RequestState::Confirmed, wv_clone.id);
                            updated = true;
                        }
                    },
                    RequestState::Finished => {
                        if alive_elevators.is_subset(&wv_clone.hall_requests[floor][dir].acks) {
                            wv_clone.hall_requests[floor][dir].set_to(RequestState::None, wv_clone.id);
                            updated = true;
                        }
                    },
                    _ => ()
                }
            }
        }

        // go through cab_requests
        for (_, elev) in wv_clone.elevators.iter_mut() {
            for floor in 0..config::FLOOR_COUNT {
                match elev.cab_requests[floor].state {
                    RequestState::Unconfirmed => {
                        if alive_elevators.is_subset(&elev.cab_requests[floor].acks) {
                            elev.cab_requests[floor].set_to(RequestState::Confirmed, wv_clone.id);
                            updated = true;
                        }
                    },
                    RequestState::Finished => {
                        if alive_elevators.is_subset(&elev.cab_requests[floor].acks) {
                            elev.cab_requests[floor].set_to(RequestState::None, wv_clone.id);
                            updated = true;
                        }
                    },
                    _ => ()
                }
            }
        }
        (wv_clone, updated)
    }

    /// Handles a button press event
    /// Returns updated WorldView and whether changes were made
    pub fn handle_button_press(&self, button_press: &CallButton) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let mut updated = false;
        match button_press.call {
            0|1 => { // hall_request?
                match wv_clone.hall_requests[button_press.floor as usize][button_press.call as usize].state {
                    RequestState::None => {
                        info!("ButtonPress(Floor: {}, Type: {})", button_press.floor, button_press.call);
                        wv_clone.hall_requests[button_press.floor as usize][button_press.call as usize].set_to(RequestState::Unconfirmed, wv_clone.id);
                        updated = true;
                    },
                    _ => (),
                }
            },
            2 => { // cab_request?
                let own_elev = wv_clone.elevators.get_mut(&wv_clone.id).expect("key should have been available");
                match own_elev.cab_requests[button_press.floor as usize].state {
                    RequestState::None => {
                        own_elev.cab_requests[button_press.floor as usize].set_to(RequestState::Unconfirmed, wv_clone.id);
                        updated = true;
                    },
                    _ => (),
                }
            },
            _ => () // unknown request
        }
        (wv_clone, updated)
    }

    /// Updates this elevator's state
    /// Returns updated WorldView and whether changes were made
    pub fn handle_elevator_state(&self, dirn: Dirn, behaviour: ElevatorBehaviour, floor: i8) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let elev = wv_clone.elevators.get_mut(&wv_clone.id).expect("key should have been available");
        if elev.state.current_floor != floor || elev.state.behaviour != behaviour 
        {
            if !elev.is_working
            {
                info!("Own elevator recovered");
            }
            elev.last_moved = SystemTime::now();
            elev.is_working = true;
        }
        elev.state.dirn = dirn;
        elev.state.behaviour = behaviour;
        elev.state.current_floor = floor;
        (wv_clone, true)
    }

    /// Clears specified requests for a floor
    /// Returns updated WorldView and whether changes were made
    pub fn handle_clear_request(&self, floor: usize, should_clear: &[bool; 3]) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let own_elev = wv_clone.elevators.get_mut(&wv_clone.id).expect("key should have been available");
        debug!("Clearing {:?}", &should_clear);
        for i in 0..2 {
            if should_clear[i] {
                wv_clone.hall_requests[floor][i].set_to(RequestState::Finished, wv_clone.id);
            }
        }

        if should_clear[2] {
            own_elev.cab_requests[floor].set_to(RequestState::Finished, wv_clone.id);
        }
        (wv_clone, true)
    }

    /// Gets set of elevators considered alive
    pub fn get_alive_elevators(&self, timeout: u64) -> HashSet<u8> {
        let mut alive_elevators = HashSet::new();
        for (id, elev) in self.elevators.iter() {
            if 
            (*id != self.id) && 
                ((elev.last_received.elapsed().expect("elapsed() failed")) > Duration::from_secs(timeout))
            {continue;}
            alive_elevators.insert(*id);
        }
        debug!("AliveElevators({timeout}): {:?}", alive_elevators);
        alive_elevators
    }

    /// Gets this elevator's ID
    pub fn get_id(&self) -> u8 {
        self.id
    }

    /// Gets all known elevators
    pub fn get_elevators(&self) -> HashMap<u8, Elevator> {
        self.elevators.clone()
    }

    /// Assigns requests to elevators using the cost algorithm
    /// Returns controller requests and list of active elevator IDs
    pub fn assign_requests(&self) -> (fsm::ControllerRequests, Vec<i32>) {
        let result: Option<(fsm::ControllerRequests, Vec<i32>)> = cost::elevator_algorithm(&self);
        match result {
            Some((mut r, active_elevators)) => {
                let tmp = self.get_confirmed_requests();
                for floor in 0..config::FLOOR_COUNT {
                    r[floor][2] = tmp[floor][2];  
                }
                (r, active_elevators)
            },
            None => {
                error!("Elevator Algorithm failed");
                (self.get_confirmed_requests(), vec![])
            }
        }
    }

    /// Gets all confirmed requests in boolean matrix form
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

    /// Gets all hall requests
    pub fn get_hall_requests(&self) -> [[Request; 2]; config::FLOOR_COUNT] {
        self.hall_requests.clone()
    }
}

/// Main manager loop that coordinates the elevator system
///
/// # Parameters
/// - `id`: This elevator's ID
/// - `manager_rx`: Channel for receiving manager messages
/// - `sender_tx`: Channel for sending network messages
/// - `controller_tx`: Channel for sending controller commands
/// - `lights_tx`: Channel for sending light updates
/// - `call_button_rx`: Channel for receiving button presses
/// - `alarm_rx`: Channel for receiving alarm signals
///
/// # Behavior
/// 1. Maintains the WorldView state
/// 2. Handles incoming messages and updates state accordingly
/// 3. Coordinates with other elevators
/// 4. Detects and handles failures
/// 5. Runs indefinitely
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
    let mut network_available = true;
    let mut humble_counter = 5;
    loop {
        let mut updated = false;
        cbc::select! {
            recv(manager_rx) -> a => {
                let message = a.expect("couldn't get message");
                match message {
                    messages::Manager::Ping(id) => {
                        debug!("Received Ping({})", id);
                        network_available = true;
                        if world_view.get_id() != id {
                            sender_tx.send(messages::Manager::Pong(world_view.get_id())).expect("send to sender failed");
                        }
                    },
                    messages::Manager::Pong(id) => {
                        debug!("Received Pong({})", id);
                        network_available = true;
                    },
                    messages::Manager::NetworkError => {
                        debug!("Received NetworkError");
                        network_available = false;
                    },
                    messages::Manager::HeartBeat(_, foreign_world_view) => {
                        debug!("Received WorldView");
                        network_available = true;
                        if foreign_world_view.get_id() != world_view.get_id() {
                            if humble_counter > 0 {
                                let new_wv = world_view.handle_humbly(foreign_world_view);
                                world_view = new_wv;
                                humble_counter = 0;
                            } else if !network_available {
                                let new_wv = world_view.handle_network_recovery(foreign_world_view);
                                world_view = new_wv;
                                updated = true;
                                network_available = true;
                            } else {
                                let (new_wv, up) = world_view.handle_foreign_world_view(foreign_world_view);
                                if up {
                                    world_view.compare_world_views(&new_wv);
                                }
                                world_view = new_wv;                                   
                                updated = up;
                            }
                        } else {
                            debug!("RECEIVED FROM MYSELF");
                        }
                    },
                    messages::Manager::ElevatorState(dirn, behaviour, floor) => {
                        debug!("Received ElevatorState");
                        let (new_wv, up) = world_view.handle_elevator_state(dirn, behaviour, floor);
                        if up {
                            world_view.compare_world_views(&new_wv);
                            world_view = new_wv;
                        }
                        updated = true;
                    },
                    messages::Manager::ClearRequest(floor, should_clear) => {
                        debug!("Received ClearRequest");
                        let (new_wv, up) = world_view.handle_clear_request(floor, &should_clear);
                        if up {
                            world_view.compare_world_views(&new_wv);
                            world_view = new_wv;
                        }
                        updated = up;
                    }
                }
            },
            recv(call_button_rx) -> a => {
                debug!("Received ButtonPress");
                let button_press = a.expect("couldn't get message");
                if humble_counter == 0 && network_available ||
                button_press.call == 2 {
                    let (new_wv, up) = world_view.handle_button_press(&button_press);
                    if up {
                        world_view.compare_world_views(&new_wv);
                        world_view = new_wv;
                    }
                    updated = up;
                }
            },
            recv(alarm_rx) -> _a => {
                debug!("Received Alarm");
                debug!("network_available: {}, humble_counter: {}", network_available, humble_counter);
                if !network_available {
                    sender_tx.send(messages::Manager::Ping(world_view.get_id())).expect("send to sender failed");
                } else if humble_counter > 0 {
                    humble_counter -= 1;
                } 
                let (new_wv, up) = world_view.update_states_at_barrier();
                if up {
                    world_view.compare_world_views(&new_wv);
                    world_view = new_wv;
                }
                updated = true;
                
                for (id, elevator) in world_view.elevators.iter_mut() {
                    if !elevator.has_request{
                        elevator.last_moved = SystemTime::now();
                    }
                    if elevator.has_request && elevator.last_moved.elapsed().expect("elapsed() failed") > Duration::from_secs(10) {
                        if elevator.is_working {
                            info!("Elevator {} is not working", id);
                            updated = true;
                        }
                        elevator.is_working = false; 
                    }
                } 
            }
        }

        if updated {
            if humble_counter <= 0 && network_available {
                let world_view_clone = world_view.clone();
                sender_tx.send(messages::Manager::HeartBeat(std::time::SystemTime::now(), world_view_clone)).expect("send to sender failed");
            }
            for elevator in world_view.elevators.values_mut() {
                elevator.has_request = false;
            }
            let (controller_reqs, active_elevators) = world_view.assign_requests(); 
            for (id, elevator) in world_view.elevators.iter_mut() {
                if active_elevators.contains(&(*id as i32)) {
                    elevator.has_request = true;

                    // if already has_request, do not reset counter
                } else {
                    elevator.last_moved = SystemTime::now();
                }
            }
            controller_tx.send(messages::Controller::Requests(controller_reqs)).expect("send to controller failed");
            
            let lights_reqs = world_view.get_confirmed_requests();
            lights_tx.send(messages::Controller::Requests(lights_reqs)).expect("send to lights failed");
        }
    }
}


// /// Tests for the Manager module
// mod test_manager_functions {
//     use std::thread;

//     use super::*;

//     // Tests that a Ping message is handled without crashing
//     #[test]
//     #[ignore = "Requires to run more elevators"]
//     fn test_ping_message_handling() {
//         let (manager_tx, manager_rx) = cbc::unbounded();
//         let (sender_tx, _) = cbc::unbounded();
//         let (controller_tx, _) = cbc::unbounded();
//         let (lights_tx, _) = cbc::unbounded();
//         let (_call_button_tx, call_button_rx) = cbc::unbounded();
//         let (_alarm_tx, alarm_rx) = cbc::unbounded();

//         manager_tx.send(messages::Manager::Ping).unwrap();
    
//         let result = std::panic::catch_unwind(|| {
//             run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//         });
        
//         assert!(result.is_ok(), "Function panicked during Ping handling");
//     }

//     // Tests HeartBeat message handling in humble mode
//     #[test]
//     #[ignore = "Requires to run more elevators"]
//     fn test_humble_mode_heartbeat() {
//         let (manager_tx, manager_rx) = cbc::unbounded();
//         let (sender_tx, sender_rx) = cbc::unbounded();
//         let (controller_tx, _) = cbc::unbounded();
//         let (lights_tx, _) = cbc::unbounded();
//         let (_call_button_tx, call_button_rx) = cbc::unbounded();
//         let (_alarm_tx, alarm_rx) = cbc::unbounded();

//         let foreign_view = WorldView::init(2);
//         manager_tx.send(messages::Manager::HeartBeat(foreign_view.clone())).unwrap();

//         std::thread::spawn(move || {
//             run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//         });

//         // Verify humble mode handled the message
//         assert!(sender_rx.try_recv().is_ok());
//     }


//     // Tests button press handling after humble mode ends
//     #[test]
//     #[ignore = "Requires to run more elevators"]
//     fn test_button_press_after_humble() {
//         let (_manager_tx, manager_rx) = cbc::unbounded();
//         let (sender_tx, sender_rx) = cbc::unbounded();
//         let (controller_tx, _) = cbc::unbounded();
//         let (lights_tx, _) = cbc::unbounded();
//         let (call_button_tx, call_button_rx) = cbc::unbounded();
//         let (alarm_tx, alarm_rx) = cbc::unbounded();

//         // Send 5 alarms to exit humble mode
//         for _ in 0..5 {
//             alarm_tx.send(1).unwrap();
//         }

//         // Send button press
//         call_button_tx.send(CallButton { floor: 1, call: 0 }).unwrap();

//         std::thread::spawn(move || {
//             run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//         });

//         // Verify button press was processed
//         assert!(sender_rx.try_recv().is_ok());
//     }


//     // Tests alarm handling and dead counter decrement
//     #[test]
//     #[ignore = "Requires to run more elevators"]
//     fn test_alarm_handling_with_dead_counter() {
//         let (_manager_tx, manager_rx) = cbc::unbounded();
//         let (sender_tx, sender_rx) = cbc::unbounded();
//         let (controller_tx, _) = cbc::unbounded();
//         let (lights_tx, _) = cbc::unbounded();
//         let (_call_button_tx, call_button_rx) = cbc::unbounded();
//         let (alarm_tx, alarm_rx) = cbc::unbounded();

//         alarm_tx.send(1).unwrap();

//         std::thread::spawn(move || {
//             run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//         });

//         // Verify alarm was processed
//         assert!(sender_rx.try_recv().is_ok());
//     }

//     // Tests that HeartBeat message is sent with correct WorldView
//     #[test]
//     #[ignore = "Requires to run more elevators"]
//     fn test_sends_heartbeat() {
//         let mut world = WorldView::init(1);
     
//         // Create all channels with small buffers
//         let (sender_tx, sender_rx) = cbc::bounded(1);
//         let (controller_tx, controller_rx) = cbc::bounded(1);
//         let (lights_tx, lights_rx) = cbc::bounded(1);

//         // Spawn thread to handle controller messages
//         let controller_handler = thread::spawn(move || {
//             match controller_rx.recv_timeout(Duration::from_millis(100)) {
//                 Ok(messages::Controller::Requests(_)) => (),
//                 _ => panic!("Controller channel error"),
//             }
//         });

//         // Spawn thread to handle lights messages
//         let lights_handler = thread::spawn(move || {
//             match lights_rx.recv_timeout(Duration::from_millis(100)) {
//                 Ok(messages::Controller::Requests(_)) => (),
//                 _ => panic!("Lights channel error"),
//             }
//         });

//         // Execute the function under test
//         inform_everybody(&mut world, &sender_tx, &controller_tx, &lights_tx);

//         // Verify the heartbeat message
//         match sender_rx.recv_timeout(Duration::from_millis(100)) {
//             Ok(messages::Manager::HeartBeat(wv)) => assert_eq!(wv.id, 1),
//             _ => panic!("Expected HeartBeat message"),



//         // Ensure all handlers completed
//         controller_handler.join().expect("Controller handler failed");
//         lights_handler.join().expect("Lights handler failed");
//         }
// // Checks that run processes messages correctly
//             #[test]
//             #[ignore = "Requires to run more elevators"]
//             fn test_run() {
//                 let (manager_tx, manager_rx) = cbc::unbounded();
//                 let (sender_tx, sender_rx) = cbc::unbounded();
//                 let (controller_tx, controller_rx) = cbc::unbounded();
//                 let (lights_tx, lights_rx) = cbc::unbounded();
//                 let (call_button_tx, call_button_rx) = cbc::unbounded();
//                 let (alarm_tx, alarm_rx) = cbc::unbounded();
        
//                 let id = 1;
//                 let manager_handle = std::thread::spawn(move || {
//                     run(id, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//                 });
        
//                 manager_tx.send(messages::Manager::Ping).unwrap();
//                 manager_tx.send(messages::Manager::HeartBeat(WorldView::init(2))).unwrap();
//                 manager_tx.send(messages::Manager::ElevatorState(Dirn::Up, ElevatorBehaviour::Moving, 3)).unwrap();
//                 manager_tx.send(messages::Manager::ClearRequest(2, [true, false, true])).unwrap();
        
//                 let button = CallButton { floor: 2, call: 0 };
//                 call_button_tx.send(button).unwrap();
        
//                 alarm_tx.send(1).unwrap();
        
//                 manager_handle.join().unwrap();
//             }

//             // Checks that run processes messages correctly
//             #[test]
//             #[ignore = "Requires to run more elevators"]
//             fn test_run_humble() {
//                 let (manager_tx, manager_rx) = cbc::unbounded();
//                 let (sender_tx, sender_rx) = cbc::unbounded();
//                 let (controller_tx, controller_rx) = cbc::unbounded();
//                 let (lights_tx, lights_rx) = cbc::unbounded();
//                 let (call_button_tx, call_button_rx) = cbc::unbounded();
//                 let (alarm_tx, alarm_rx) = cbc::unbounded();
        
//                 let id = 1;
//                 let manager_handle = std::thread::spawn(move || {
//                     run(id, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//                 });
        
//                 manager_tx.send(messages::Manager::Ping).unwrap();
//                 manager_tx.send(messages::Manager::HeartBeat(WorldView::init(2))).unwrap();
//                 manager_tx.send(messages::Manager::ElevatorState(Dirn::Up, ElevatorBehaviour::Moving, 3)).unwrap();
//                 manager_tx.send(messages::Manager::ClearRequest(2, [true, false, true])).unwrap();
        
//                 let button = CallButton { floor: 2, call: 0 };
//                 call_button_tx.send(button).unwrap();
        
//                 alarm_tx.send(1).unwrap();
        
//                 manager_handle.join().unwrap();
//             }

//             // Checks that run processes messages correctly
//             #[test]
//             #[ignore = "Requires to run more elevators"]
//             fn test_run_foreign() {
//                 let (manager_tx, manager_rx) = cbc::unbounded();
//                 let (sender_tx, sender_rx) = cbc::unbounded();
//                 let (controller_tx, controller_rx) = cbc::unbounded();
//                 let (lights_tx, lights_rx) = cbc::unbounded();
//                 let (call_button_tx, call_button_rx) = cbc::unbounded();
//                 let (alarm_tx, alarm_rx) = cbc::unbounded();
        
//                 let id = 1;
//                 let manager_handle = std::thread::spawn(move || {
//                     run(id, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
//                 });
        
//                 manager_tx.send(messages::Manager::Ping).unwrap();
//                 manager_tx.send(messages::Manager::HeartBeat(WorldView::init(2))).unwrap();
//                 manager_tx.send(messages::Manager::ElevatorState(Dirn::Up, ElevatorBehaviour::Moving, 3)).unwrap();
//                 manager_tx.send(messages::Manager::ClearRequest(2, [true, false, true])).unwrap();
        
//                 let button = CallButton { floor: 2, call: 0 };
//                 call_button_tx.send(button).unwrap();
        
//                 alarm_tx.send(1).unwrap();
        
//                 manager_handle.join().unwrap();
//             }
//     // Helper function to drain a channel
//     fn drain_channel<T>(rx: &cbc::Receiver<T>) {
//         while let Ok(_) = rx.try_recv() {}
//         }

//     // Tests that active elevators get has_request set correctly
//     #[test]
//     #[ignore = "Requires to run more elevators"]
//     fn test_updates_active_elevators() {
//         let world = WorldView::init(1);
//         let (sender_tx, sender_rx) = cbc::unbounded();
//         let (controller_tx, controller_rx) = cbc::unbounded();
//         let (lights_tx, lights_rx) = cbc::unbounded();

//         // Spawn consumers for all channels
//         thread::spawn(move || {
//             drain_channel(&sender_rx);
//             drain_channel(&controller_rx);
//             drain_channel(&lights_rx);
//             });

//         let mut mock_world = world.clone();
//         mock_world.elevators.get_mut(&1).unwrap().has_request = false;
        
//         inform_everybody(&mut mock_world, &sender_tx, &controller_tx, &lights_tx);

//         let elev = mock_world.elevators.get(&1).unwrap();
//         assert!(elev.has_request);
//         assert_eq!(elev.detect_if_dead_counter, 10);
// }

