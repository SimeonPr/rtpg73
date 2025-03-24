use driver_rust::elevio::poll::CallButton;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

use crate::config;
use crate::cost;
use crate::fsm;
use crate::fsm::{Dirn, ElevatorBehaviour};
use crate::messages;
use crossbeam_channel as cbc;
use driver_rust::elevio;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub enum RequestState {
    #[default]
    None = 0,
    Unconfirmed = 1,
    // Barrier
    Confirmed = 2,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Request {
    state: RequestState,
    acks: HashSet<u8>,
}

impl Request {
    pub fn new(id: u8) -> Request {
        let mut hs = HashSet::new();
        hs.insert(id);
        Request {
            state: RequestState::None,
            acks: hs,
        }
    }

    pub fn set_to(&mut self, r: RequestState, id: u8) {
        self.state = r;
        self.acks = HashSet::new();
        self.acks.insert(id);
    }

    /// Merge self with r2, adding id to acknowledgements.
    /// Returns true if the state changed.
    pub fn merge(&mut self, r2: &Request, id: u8) -> bool {
        let mut updated = false;
        let new_state = match self.state {
            RequestState::None => match r2.state {
                RequestState::Unconfirmed => RequestState::Unconfirmed,
                _ => self.state,
            },
            RequestState::Unconfirmed => match r2.state {
                RequestState::Confirmed => RequestState::Confirmed,
                _ => self.state,
            },
            RequestState::Confirmed => match r2.state {
                RequestState::None => RequestState::None,
                _ => self.state,
            },
        };
        if self.state != new_state {
            updated = true;
            self.state = new_state;
            self.acks = r2.acks.clone();
            self.acks.insert(id);
        } else if self.state == r2.state {
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
    pub dirn: Dirn,
    pub behaviour: ElevatorBehaviour,
    pub current_floor: i8,
}

impl ElevatorNetworkState {
    pub fn new() -> ElevatorNetworkState {
        ElevatorNetworkState {
            dirn: Dirn::Stop,
            behaviour: ElevatorBehaviour::Idle,
            current_floor: -1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Elevator {
    last_received: SystemTime,
    pub state: ElevatorNetworkState,
    cab_requests: [Request; config::FLOOR_COUNT],
    has_request: bool,
    detect_if_dead_counter: u8,
    is_working: bool,
}

impl Elevator {
    pub fn new() -> Elevator {
        let cab_requests = Default::default();
        Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState::new(),
            cab_requests,
            has_request: false,
            detect_if_dead_counter: 10,
            is_working: true,
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
    hall_requests: [[Request; 2]; config::FLOOR_COUNT],
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
            hall_requests,
        }
    }

    pub fn compare_world_views(&self, other: &WorldView) {
        // Hall requests
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if self.hall_requests[floor][dir].state != other.hall_requests[floor][dir].state {
                    info!(
                        "HallRequestState(floor: {}, dir: {}): {:?} -> {:?}",
                        floor, dir, self.hall_requests[floor][dir].state, other.hall_requests[floor][dir].state
                    );
                }
            }
        }
        // New elevators?
        for key in other.elevators.keys() {
            if !self.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
            }
        }
        // Compare cab requests + network state
        for (id, elev) in self.elevators.iter() {
            if let Some(other_elev) = other.elevators.get(id) {
                for floor in 0..config::FLOOR_COUNT {
                    if elev.cab_requests[floor].state != other_elev.cab_requests[floor].state {
                        info!(
                            "CabRequestState(id: {}, floor: {}, type: {}): {:?} -> {:?}",
                            id,
                            floor,
                            2,
                            elev.cab_requests[floor].state,
                            other_elev.cab_requests[floor].state
                        );
                    }
                }
                if elev.state.dirn != other_elev.state.dirn {
                    info!(
                        "Dirn(id: {}): {:?} -> {:?}",
                        id, elev.state.dirn, other_elev.state.dirn
                    );
                }
                if elev.state.current_floor != other_elev.state.current_floor {
                    info!(
                        "CurrentFloor(id: {}): {} -> {}",
                        id, elev.state.current_floor, other_elev.state.current_floor
                    );
                }
                if elev.state.behaviour != other_elev.state.behaviour {
                    info!(
                        "Behaviour(id: {}): {:?} -> {:?}",
                        id, elev.state.behaviour, other_elev.state.behaviour
                    );
                }
            } else {
                info!("ID {} removed", id);
            }
        }
    }

    
    pub fn handle_humbly(&self, foreign_world_view: WorldView) -> WorldView {
        let mut new_wv = foreign_world_view;
        let id = self.id;
        if !new_wv.elevators.contains_key(&id) {
            if let Some(old_elev) = self.elevators.get(&id) {
                new_wv.elevators.insert(id, old_elev.clone());
            }
        }
        new_wv.id = id;
        new_wv
    }

    
    pub fn handle_foreign_world_view(&self, foreign_world_view: WorldView) -> (WorldView, bool) {
        let mut new_wv = self.clone();
        let mut updated = false;
        let current_time = SystemTime::now();
        let foreign_id = foreign_world_view.get_id();
        let foreign_elevators = foreign_world_view.get_elevators();

        // Add new elevators.
        for key in foreign_world_view.elevators.keys() {
            if !new_wv.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
                if let Some(u) = foreign_world_view.elevators.get(key) {
                    new_wv.elevators.insert(*key, u.clone());
                }
            }
        }

        // Update the foreign elevator state.
        if let Some(e) = foreign_elevators.get(&foreign_id) {
            if let Some(u) = new_wv.elevators.get_mut(&foreign_id) {
                let recovered = (u.state.current_floor != e.state.current_floor)
                    || (u.state.behaviour != e.state.behaviour);
                if recovered || !u.has_request{
                    if u.detect_if_dead_counter == 0{
                    info!(
                        "Foreign Elevator {} recovered, resetting detect_if_dead_counter.",
                        foreign_id
                    );
                    }
                    if recovered {u.is_working = true;}
                    u.detect_if_dead_counter = 10;
                    
                }
                u.last_received = current_time;
                u.state = e.state;
            }
        }

        // Update hall requests.
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                let res = new_wv.hall_requests[floor][dir]
                    .merge(&foreign_world_view.hall_requests[floor][dir], new_wv.id);
                updated |= res;
            }
        }

        // Update cab requests.
        for (id, foreign_elev) in foreign_elevators.iter() {
            if let Some(own_elev) = new_wv.elevators.get_mut(id) {
                for floor in 0..config::FLOOR_COUNT {
                    let res = own_elev.cab_requests[floor]
                        .merge(&foreign_elev.cab_requests[floor], new_wv.id);
                    updated |= res;
                }
            }
        }

        // Update states at barrier.
        let (new_wv, tmp_updated) = new_wv.update_states_at_barrier();
        updated |= tmp_updated;
        (new_wv, updated)
    }

   //Update our state at barriers.
    pub fn update_states_at_barrier(&self) -> (WorldView, bool) {
        let mut new_wv = self.clone();
        let mut updated = false;
        let alive_elevators = new_wv.get_alive_elevators(1);

        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if let RequestState::Unconfirmed = new_wv.hall_requests[floor][dir].state {
                    if alive_elevators.is_subset(&new_wv.hall_requests[floor][dir].acks) && !alive_elevators.is_empty() {
                        new_wv.hall_requests[floor][dir].set_to(RequestState::Confirmed, new_wv.id);
                        updated = true;
                    }
                }
            }
        }
        for (_, elev) in new_wv.elevators.iter_mut() {
            for floor in 0..config::FLOOR_COUNT {
                if let RequestState::Unconfirmed = elev.cab_requests[floor].state {
                    
                    if alive_elevators.is_subset(&elev.cab_requests[floor].acks) && !alive_elevators.is_empty() {
                        elev.cab_requests[floor].set_to(RequestState::Confirmed, new_wv.id);
                        updated = true;
                    }
                }
            }
        }
        (new_wv, updated)
    }

    
    pub fn handle_button_press(&self, button_press: &CallButton) -> (WorldView, bool) {
        let mut new_wv = self.clone();
        let mut updated = false;
        match button_press.call {
            0 | 1 => {
                if let RequestState::None =
                    new_wv.hall_requests[button_press.floor as usize][button_press.call as usize].state
                {
                    info!(
                        "ButtonPress(Floor: {}, Type: {})",
                        button_press.floor, button_press.call
                    );
                    new_wv.hall_requests[button_press.floor as usize][button_press.call as usize]
                        .set_to(RequestState::Unconfirmed, new_wv.id);
                    updated = true;
                }
            }
            2 => {
                if let Some(own_elev) = new_wv.elevators.get_mut(&new_wv.id) {
                    if let RequestState::None = own_elev.cab_requests[button_press.floor as usize].state {
                        own_elev.cab_requests[button_press.floor as usize]
                            .set_to(RequestState::Unconfirmed, new_wv.id);
                        updated = true;
                    }
                }
            }
            _ => {}
        }
        (new_wv, updated)
    }

    
    pub fn handle_elevator_state(
        &self,
        dirn: Dirn,
        behaviour: ElevatorBehaviour,
        floor: i8,
    ) -> (WorldView, bool) {
        let mut new_wv = self.clone();
        if let Some(elev) = new_wv.elevators.get_mut(&new_wv.id) {
            let recovered = (elev.state.current_floor != floor)
                || (elev.state.behaviour != behaviour);
            if recovered || !elev.has_request {
                if recovered {elev.is_working = true;}
                if elev.detect_if_dead_counter == 0 {
                    info!("Own elevator recovered, resetting detect_if_dead_counter.");
                }
                elev.detect_if_dead_counter = 10;
                
            }
            elev.state.dirn = dirn;
            elev.state.behaviour = behaviour;
            elev.state.current_floor = floor;
        }
        (new_wv, true)
    }

    
    pub fn handle_clear_request(&self, floor: usize, should_clear: &[bool; 3]) -> (WorldView, bool) {
        let mut new_wv = self.clone();
        if let Some(own_elev) = new_wv.elevators.get_mut(&new_wv.id) {
            debug!("Clearing {:?}", should_clear);
            for i in 0..2 {
                if should_clear[i] {
                    new_wv.hall_requests[floor][i].set_to(RequestState::None, new_wv.id);
                }
            }
            if should_clear[2] {
                own_elev.cab_requests[floor].set_to(RequestState::None, new_wv.id);
            }
        }
        (new_wv, true)
    }

    
    pub fn get_alive_elevators(&self, timeout: u64) -> HashSet<u8> {
        let mut alive = HashSet::new();
        for (id, elev) in self.elevators.iter() {
            if (*id != self.id)
                && (elev.last_received.elapsed().expect("elapsed() failed")
                    > Duration::from_secs(timeout) && !elev.is_working)
            {
                continue;
            }
            
            alive.insert(*id);

        }
        
        println!("Alive elevators {:?}", alive);
        
        
        alive
    }

    pub fn get_id(&self) -> u8 {
        self.id
    }

    pub fn get_elevators(&self) -> HashMap<u8, Elevator> {
        self.elevators.clone()
    }

    pub fn assign_requests(&self) -> (fsm::ControllerRequests, Vec<i32>) {
        let result: Option<(fsm::ControllerRequests, Vec<i32>)> = cost::elevator_algorithm(self);
        match result {
            Some((mut r, active_elevators)) => {
                let tmp = self.get_confirmed_requests();
                for floor in 0..config::FLOOR_COUNT {
                    r[floor][2] = tmp[floor][2];
                }
                (r, active_elevators)
            }
            None => {
                error!("Elevator Algorithm failed");
                (self.get_confirmed_requests(), vec![])
            }
        }
    }

    pub fn get_confirmed_requests(&self) -> [[bool; config::CALL_COUNT]; config::FLOOR_COUNT] {
        let mut requests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];
        if let Some(elev) = self.elevators.get(&self.id) {
            for floor in 0..config::FLOOR_COUNT {
                for dir in 0..2 {
                    if let RequestState::Confirmed = self.hall_requests[floor][dir].state {
                        requests[floor][dir] = true;
                    }
                }
                if let RequestState::Confirmed = elev.cab_requests[floor].state {
                    requests[floor][2] = true;
                }
            }
        }
        requests
    }

    pub fn get_hall_requests(&self) -> [[Request; 2]; config::FLOOR_COUNT] {
        self.hall_requests.clone()
    }
}



fn update_dead_elevators(
    world_view: WorldView
) -> (WorldView, bool, fsm::ControllerRequests, Vec<i32>) {
    let mut new_wv = world_view;
    let mut changed = false;
    
    // First, decrement dead counters if a request is pending.
    for (_id, elevator) in new_wv.elevators.iter_mut() {
        let old_counter = elevator.detect_if_dead_counter;
        let old_working = elevator.is_working;
        if elevator.has_request && elevator.detect_if_dead_counter > 0 {
            elevator.detect_if_dead_counter -= 1;
        }
        if elevator.detect_if_dead_counter == 0 && elevator.is_working {
            elevator.is_working = false;
        }
        if elevator.detect_if_dead_counter != old_counter || elevator.is_working != old_working {
            changed = true;
        }
    }
    
    // Get the current controller requests and active elevator IDs.
    let (controller_reqs, active_elevators) = new_wv.assign_requests();
    
    // Then, reassign call flags based on active elevators.
    for (id, elevator) in new_wv.elevators.iter_mut() {
        let old_has_request = elevator.has_request;
        let old_counter = elevator.detect_if_dead_counter;
        if active_elevators.contains(&(*id as i32)) {
            if !elevator.has_request {
                elevator.has_request = true;
                elevator.detect_if_dead_counter = 10; // reset counter for active elevators
            }
        } else {
            elevator.has_request = false;
            if elevator.detect_if_dead_counter > 0 {
                elevator.detect_if_dead_counter = 10;
            }
        }
        if elevator.has_request != old_has_request || elevator.detect_if_dead_counter != old_counter {
            changed = true;
        }
    }
    
    (new_wv, changed, controller_reqs, active_elevators)
}


fn prepare_inform_messages(world_view: WorldView) -> (messages::Manager, messages::Controller, messages::Controller) {
    let heartbeat = messages::Manager::HeartBeat(world_view.clone());
    let (controller_reqs, active_elevators) = world_view.assign_requests();
    let mut updated_wv = world_view.clone();
    for (id, elevator) in updated_wv.elevators.iter_mut() {
        if active_elevators.contains(&(*id as i32)) {
            if !elevator.has_request {
                elevator.has_request = true;
                elevator.detect_if_dead_counter = 10;
            }
        } else {
            elevator.has_request = false;
            if elevator.detect_if_dead_counter > 0 {
                elevator.detect_if_dead_counter = 10;
            }
        }
    }
    let controller_msg = messages::Controller::Requests(controller_reqs);
    let lights_reqs = updated_wv.get_confirmed_requests();
    let lights_msg = messages::Controller::Requests(lights_reqs);
    (heartbeat, controller_msg, lights_msg)
}

/// Main run loop.

pub fn run(
    id: u8,
    manager_rx: cbc::Receiver<messages::Manager>,
    sender_tx: cbc::Sender<messages::Manager>,
    controller_tx: cbc::Sender<messages::Controller>,
    lights_tx: cbc::Sender<messages::Controller>,
    call_button_rx: cbc::Receiver<elevio::poll::CallButton>,
    alarm_rx: cbc::Receiver<u8>,
) {
    info!("Manager up and running...");
    let mut world_view = WorldView::init(id);
    let mut humble_counter = 5;

    loop {
        let mut updated = false;
        
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
                            if humble_counter > 0 {
                                world_view = world_view.handle_humbly(foreign_world_view);
                                humble_counter -=1;
                            } else {
                                let (new_wv, up) = world_view.handle_foreign_world_view(foreign_world_view);
                                if up {
                                    world_view.compare_world_views(&new_wv);
                                    world_view = new_wv;
                                }
                                updated |= up;
                            }
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
                if humble_counter == 0 {
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
                if humble_counter > 0 {
                    humble_counter -= 1;
                } else {
                    let (new_wv, up_barrier) = world_view.update_states_at_barrier();
                    if up_barrier {
                        world_view.compare_world_views(&new_wv);
                        world_view = new_wv;
                        updated |= true;
                    }
                }
                // Update dead counters and reassign calls.
                let (new_wv, dead_changed, _controller_reqs, _active_elevators) = update_dead_elevators(world_view);
                world_view = new_wv;
                if dead_changed {
                    updated |= true;
                }
              
            }
        } // end select

        // Only send messages when humble logic allows.
        if updated && humble_counter == 0{
            
            let (heartbeat, controller_msg, lights_msg) = prepare_inform_messages(world_view.clone());
            sender_tx.send(heartbeat).expect("send to sender failed");
            controller_tx.send(controller_msg).expect("send to controller failed");
            lights_tx.send(lights_msg).expect("send to lights failed");
            alarm = false;
        }
    }
}
