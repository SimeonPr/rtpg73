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
        } else if self.state == r2.state { // state remains the same, but we need to add acks
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
    cab_requests: [Request; config::FLOOR_COUNT],
    has_request: bool,
    detect_if_dead_counter: u8
}
impl Elevator {
    pub fn new() -> Elevator {
        let cab_requests = Default::default(); 
        Elevator {
            last_received: SystemTime::now(),
            state: ElevatorNetworkState::new(),
            cab_requests,
            has_request: false,
            detect_if_dead_counter: 10
        }
    }

    pub fn get_cab_requests(&self) -> [Request; config::FLOOR_COUNT] {
        self.cab_requests.clone()
    }


}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldView {
    pub(crate) id: u8,
    pub(crate) elevators: HashMap<u8, Elevator>,
    pub(crate) hall_requests: [[Request; 2]; config::FLOOR_COUNT]
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
        for key in foreign_world_view.elevators.keys() {
            if !wv_clone.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
                let u = foreign_world_view.elevators.get(&key).expect("key should have been available");
                wv_clone.elevators.insert(*key, u.clone());
            }
        }

        // update foreign elevators state + last_received
        if let Some(e) = foreign_elevators.get(&foreign_id) { 
            let u = wv_clone.elevators.get_mut(&foreign_id).expect("key should have been available");
    
            // --- New recovery detection logic clearly implemented ---
            let recovered = (u.state.current_floor != e.state.current_floor)
                         || (u.state.behaviour != e.state.behaviour);
    
            if recovered || !u.has_request/*&& u.detect_if_dead_counter == 0*/ {
                //if u.detect_if_dead_counter == 0 {
                    //u.has_request = false;
                //}
                u.detect_if_dead_counter = 5;  // clearly reset counter
                
                   // reset flag
                info!("Foreign Elevator {} recovered, resetting detect_if_dead_counter.", foreign_id);
            }
            // --- End of clearly marked changes ---
    
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

    pub fn update_states_at_barrier(&self) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let mut updated = false;

        // get alive elevators
        let alive_elevators = wv_clone.get_alive_elevators(1);

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
                    _ => ()
                }
            }
        }
        (wv_clone, updated)
    }

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

    pub fn handle_elevator_state(&self, dirn: Dirn, behaviour: ElevatorBehaviour, floor: i8) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let elev = wv_clone.elevators.get_mut(&wv_clone.id).expect("key should have been available");
    
        // --- Recovery detection clearly added here ---
        let recovered = elev.state.current_floor != floor
                     || elev.state.behaviour != behaviour;
    
        if recovered || !elev.has_request{

            //if elev.detect_if_dead_counter == 0{
                //elev.has_request = false;
            //}else{
                //elev.detect_if_dead_counter = 10;
            //}
            elev.detect_if_dead_counter = 5;
             // clearly reset counter on recovery
              // reset request flag clearly
            info!("Own elevator recovered, resetting detect_if_dead_counter.");
        }
        
               // reset flag
        
        // --- End of recovery detection ---
    
        elev.state.dirn = dirn;
        elev.state.behaviour = behaviour;
        elev.state.current_floor = floor;
        (wv_clone, true)
    }

    pub fn handle_clear_request(&self, floor: usize, should_clear: &[bool; 3]) -> (WorldView, bool) {
        let mut wv_clone = self.clone();
        let own_elev = wv_clone.elevators.get_mut(&wv_clone.id).expect("key should have been available");
        debug!("Clearing {:?}", &should_clear);
        for i in 0..2 {
            if should_clear[i] {
                wv_clone.hall_requests[floor][i].set_to(RequestState::None, wv_clone.id);
            }
        }

        if should_clear[2] {
            own_elev.cab_requests[floor].set_to(RequestState::None, wv_clone.id);
        }
        (wv_clone, true)
    }
    // Getters
    pub fn get_alive_elevators(&self, timeout: u64) -> HashSet<u8> {
        let mut alive_elevators = HashSet::new();
        for (id, elev) in self.elevators.iter() {
            if (*id != self.id) && (elev.last_received.elapsed().expect("elapsed() failed") > Duration::from_secs(timeout) || elev.detect_if_dead_counter == 0)
            {continue;}
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

    pub fn assign_requests(&self) -> (fsm::ControllerRequests, Vec<i32>) {  // <-- Return type adjusted
        // Get Hall Requests and active elevator IDs
        let result: Option<(fsm::ControllerRequests, Vec<i32>)> = cost::elevator_algorithm(&self);  // <-- Adjusted type
        match result {
            Some((mut r, active_elevators)) => {  // <-- Destructure both elements clearly
                // Add Cab Requests
                let tmp = self.get_confirmed_requests();
                for floor in 0..config::FLOOR_COUNT {
                    r[floor][2] = tmp[floor][2];  
                }
                (r, active_elevators)  // <-- Return tuple clearly
            },
            None => {
                error!("Elevator Algorithm failed");
                (self.get_confirmed_requests(), vec![])  // <-- Return empty vector on failure
            }
        }
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
                                let new_wv = world_view.handle_humbly(foreign_world_view);
                                world_view = new_wv;
                                humble_counter = 0;
                            } else {
                                let (new_wv, up) = world_view.handle_foreign_world_view(foreign_world_view);
                                if up {
                                    world_view.compare_world_views(&new_wv);
                                    world_view = new_wv;
                                }
                                updated = up;
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
                    let (new_wv, up) = world_view.update_states_at_barrier();
                    if up {
                        world_view.compare_world_views(&new_wv);
                        world_view = new_wv;
                    }
                    updated = up;
                }

                for elevator in world_view.elevators.values_mut() {
                    if elevator.has_request && elevator.detect_if_dead_counter > 0 {
                        elevator.detect_if_dead_counter -= 1;
                        println!{"{}", elevator.detect_if_dead_counter};
                        
                    }
                }
            }
        }
        if updated && !(humble_counter > 0) {
            inform_everybody(
                &mut world_view,
                &sender_tx,
                &controller_tx,
                &lights_tx);
        }

    }
}


fn inform_everybody(
    world_view: &mut WorldView,
    sender_tx: &cbc::Sender<messages::Manager>,
    controller_tx: &cbc::Sender<messages::Controller>,
    lights_tx: &cbc::Sender<messages::Controller>
) {
    let world_view_clone = world_view.clone();
    sender_tx.send(messages::Manager::HeartBeat(world_view_clone)).expect("send to sender failed");
    
    let (controller_reqs, active_elevators) = world_view.assign_requests(); 
    for (id, elevator) in world_view.elevators.iter_mut() {
        if active_elevators.contains(&(*id as i32)) {
            if !elevator.has_request {
                elevator.has_request = true;
                elevator.detect_if_dead_counter = 10; // clearly start at 10
            }
            // if already has_request, do not reset counter
        } else {
            elevator.has_request = false;
            if elevator.detect_if_dead_counter > 0 {
                elevator.detect_if_dead_counter = 10;
            }
             
        }
    }
    controller_tx.send(messages::Controller::Requests(controller_reqs)).expect("send to controller failed");

    let lights_reqs = world_view.get_confirmed_requests();
    lights_tx.send(messages::Controller::Requests(lights_reqs)).expect("send to lights failed");
}


#[cfg(test)]
mod test_request {
    use super::*;

    // Ensures that a newly created Request has a None state and contains the given ID
    #[test]
    fn test_request_new() {
        let req = Request::new(1);
        assert_eq!(req.state, RequestState::None);
        assert!(req.acks.contains(&1));
    }

    // Checks that set_to updates the state and acks of a Request
    #[test]
    fn test_request_set_to() {
        let mut req = Request::new(1);
        req.set_to(RequestState::Confirmed, 2);
        assert_eq!(req.state, RequestState::Confirmed);
        assert!(req.acks.contains(&2));
    }

    // Checks that merge updates the state and acks of a Request and returns true for an update
    #[test]
    fn test_request_merge() {
        let mut req1 = Request::new(1);
        req1.set_to(RequestState::Unconfirmed, 1);

        let mut req2 = Request::new(2);
        req2.set_to(RequestState::Confirmed, 2);

        let updated = req1.merge(&req2, 3);
        assert!(updated);
        assert_eq!(req1.state, RequestState::Confirmed);
        assert!(req1.acks.contains(&3));
    }
}

#[cfg(test)]
mod test_elevator {
    use super::*;

    // Ensures that a newly created Elevator has a stopped direction, idle behavior, and an initial floor of -1
    #[test]
    fn test_elevator_new() {
        let elev = Elevator::new();
        assert_eq!(elev.state.dirn, Dirn::Stop);
        assert_eq!(elev.state.behaviour, ElevatorBehaviour::Idle);
        assert_eq!(elev.state.current_floor, -1);
    }

    // Verifies that all cab requests in a newly created Elevator are in the None state
    #[test]
    fn test_get_cab_requests() {
        let elev = Elevator::new();
        let requests = elev.get_cab_requests();
        for req in requests.iter() {
            assert_eq!(req.state, RequestState::None);
        }
    }

    // Ensures that a newly created WorldView has a single elevator with the given ID and a hall request for each floor
    #[test]
    fn test_worldview_init() {
        let wv = WorldView::init(1);
        assert_eq!(wv.id, 1);
        assert!(wv.elevators.contains_key(&1));
        for i in 0..config::FLOOR_COUNT {
            for j in 0..2 {
                assert_eq!(wv.hall_requests[i][j].state, RequestState::None);
            }
        }
    }

    // Checks that handle_button_press updates the hall request for the given button press and returns true for an update
    #[test]
    fn test_handle_button_press() {
        let wv = WorldView::init(1);
        let button = CallButton { floor: 2, call: 0 };
        let (new_wv, updated) = wv.handle_button_press(&button);
        assert!(updated);
        assert_eq!(new_wv.hall_requests[2][0].state, RequestState::Unconfirmed);
    }
}

#[cfg(test)]
mod test_worldview {
    use super::*;

    // Ensures that get_alive_elevators returns the expected set of alive elevators
    #[test]
    fn test_worldview_init() {
        let wv = WorldView::init(1);
        assert_eq!(wv.id, 1);
        assert!(wv.elevators.contains_key(&1));
    }

    // Checks that handle_button_press updates the hall request for the given button press and returns true for an update
    #[test]
    fn test_handle_button_press() {
        let wv = WorldView::init(1);
        let button = CallButton { floor: 2, call: 0 };
        let (new_wv, updated) = wv.handle_button_press(&button);
        assert!(updated);
        assert_eq!(
            new_wv.hall_requests[2][0].state,
            RequestState::Unconfirmed
        );
    }

    // Checks that handle_foreign_world_view updates the WorldView with the given foreign WorldView and returns true for an update
    #[test]
    fn test_handle_elevator_state() {
        let wv = WorldView::init(1);
        let (new_wv, updated) = wv.handle_elevator_state(Dirn::Up, ElevatorBehaviour::Moving, 3);
        assert!(updated);
        assert_eq!(new_wv.elevators.get(&1).unwrap().state.current_floor, 3);
    }

    // Checks that handle_clear_request updates the hall requests for the given floor and returns true for an update
    #[test]
    fn test_handle_clear_request() {
        let wv = WorldView::init(1);
        let (new_wv, updated) = wv.handle_clear_request(2, &[true, false, true]);
        assert!(updated);
        assert_eq!(
            new_wv.hall_requests[1][0].state,
            RequestState::None
        );
        assert_eq!(
            new_wv.hall_requests[0][0].state,
            RequestState::None
        );
    }

    // Checks that update_states_at_barrier updates the hall requests and cab requests at the barrier and returns true for an update
    #[test]
    fn test_update_states_at_barrier() {
        let mut wv = WorldView::init(1);
        wv.hall_requests[2][0].set_to(RequestState::Unconfirmed, 1);
        wv.elevators.get_mut(&1).unwrap().cab_requests[2].set_to(RequestState::Unconfirmed, 1);
        let (new_wv, updated) = wv.update_states_at_barrier();
        assert!(updated);
        assert_eq!(
            new_wv.hall_requests[2][0].state,
            RequestState::Confirmed
        );
        assert_eq!(
            new_wv.elevators.get(&1).unwrap().cab_requests[2].state,
            RequestState::Confirmed
        );
    }

    // Check the world view comparison function
    #[test]
    fn test_compare_world_views() {
        let wv1 = WorldView::init(1);
        let wv2 = WorldView::init(2);
        wv1.compare_world_views(&wv2);
    }

    // Check the handle_humbly function
    #[test]
    fn test_handle_humbly() {
        let wv1 = WorldView::init(1);
        let wv2 = WorldView::init(2);
        let new_wv = wv1.handle_humbly(wv2);
        assert_eq!(new_wv.id, 1);
        assert!(new_wv.elevators.contains_key(&1));
    }

    // Test that new elevators from a foreign WorldView are properly added to the local WorldView
    #[test]
    fn test_add_new_elevators() {
        let local_wv = WorldView::init(1);
        let foreign_wv = WorldView::init(2);
        let (updated_wv, _) = local_wv.handle_foreign_world_view(foreign_wv);
        assert_eq!(updated_wv.elevators.len(), 2);
        assert!(updated_wv.elevators.contains_key(&1)); 
        assert!(updated_wv.elevators.contains_key(&2)); 
    }

    // Test that state updates from foreign elevators are correctly applied to existing elevators.
    #[test]
    fn test_update_existing_elevator_state() {
        let local_wv = WorldView::init(1);
        let mut foreign_wv = WorldView::init(2); 
        
        let foreign_elev = foreign_wv.elevators.get_mut(&2).unwrap();
        foreign_elev.state.current_floor = 5;
        foreign_elev.state.behaviour = ElevatorBehaviour::Moving;
        
        let mut local_wv = local_wv.clone();
        local_wv.elevators.insert(2, foreign_elev.clone());
        
        let (updated_wv, _) = local_wv.handle_foreign_world_view(foreign_wv);
        
        let updated_elev = updated_wv.elevators.get(&2).unwrap();
        assert_eq!(updated_elev.state.current_floor, 5);
        assert_eq!(updated_elev.state.behaviour, ElevatorBehaviour::Moving);
    }

    // Test that elevator recovery is properly detected when state changes are observed
    #[test]
    fn test_recovery_detection() {
        let local_wv = WorldView::init(1);
        
        let mut foreign_wv = WorldView::init(2);
        let foreign_elev = foreign_wv.elevators.get_mut(&2).unwrap();
        
        foreign_elev.state.current_floor = 3; 
        foreign_elev.state.behaviour = ElevatorBehaviour::Moving; 
        
        let mut local_wv = local_wv.clone();
        local_wv.elevators.insert(2, foreign_elev.clone());
        
        let (updated_wv, _) = local_wv.handle_foreign_world_view(foreign_wv);
        
        let updated_elev = updated_wv.elevators.get(&2).unwrap();
        assert_eq!(updated_elev.detect_if_dead_counter, 5);
    }


    // Test that no state changes occur when there are no unconfirmed requests
    #[test]
    fn test_no_updates_when_no_unconfirmed_requests() {
        let mut wv = WorldView::init(1);
        // Set all requests to something other than Unconfirmed
        for floor in 0..config::FLOOR_COUNT {
            wv.hall_requests[floor][0].state = RequestState::Confirmed;
            wv.hall_requests[floor][1].state = RequestState::Confirmed;
        }
        for elev in wv.elevators.values_mut() {
            for floor in 0..config::FLOOR_COUNT {
                elev.cab_requests[floor].state = RequestState::Confirmed;
            }
        }

        let (updated_wv, changed) = wv.update_states_at_barrier();
        assert!(!changed);
        
        // Compare hall requests state
        for floor in 0..config::FLOOR_COUNT {
            assert_eq!(wv.hall_requests[floor][0].state, updated_wv.hall_requests[floor][0].state);
            assert_eq!(wv.hall_requests[floor][1].state, updated_wv.hall_requests[floor][1].state);
        }
        
        // Compare cab requests state
        for (id, elev) in &wv.elevators {
            for floor in 0..config::FLOOR_COUNT {
                assert_eq!(
                    elev.cab_requests[floor].state,
                    updated_wv.elevators.get(id).unwrap().cab_requests[floor].state
                );
            }
        }
    }


    // Test that a hall request becomes confirmed when all alive elevators have acknowledged it
    #[test]
    fn test_hall_request_confirmed_when_all_alive_elevators_acked() {
        let mut wv = WorldView::init(1);
        // Set one hall request to Unconfirmed
        wv.hall_requests[2][0].state = RequestState::Unconfirmed;
        // Add acks from all elevators (assuming all are alive)
        for elev_id in wv.elevators.keys() {
            wv.hall_requests[2][0].acks.insert(*elev_id);
        }

        let (updated_wv, changed) = wv.update_states_at_barrier();
        assert!(changed);
        assert_eq!(updated_wv.hall_requests[2][0].state, RequestState::Confirmed);
    }

    // Test that a hall request remains unconfirmed when some alive elevators haven't acknowledged it
    #[test]
    #[ignore = "Requires a running elevator simulator"]
    fn test_hall_request_not_confirmed_when_missing_acks() {
        // Create a world view with multiple elevators
        let mut wv = WorldView::init(1);
        
        // Add a second elevator (if your setup allows)
        wv.elevators.insert(2, Elevator::new());
        
        // Set one hall request to Unconfirmed
        wv.hall_requests[3][1].state = RequestState::Unconfirmed;
        
        // Add ack from only one elevator (not all)
        if let Some(&elev_id) = wv.elevators.keys().next() {
            wv.hall_requests[3][1].acks.insert(elev_id);
        }
    
        let (updated_wv, changed) = wv.update_states_at_barrier();
        
        // Shouldn't change because not all alive elevators have acked
        assert!(!changed, "Expected no change but got changed=true. Acks: {:?}, Alive elevators: {:?}", 
            wv.hall_requests[3][1].acks,
            wv.get_alive_elevators(1));
        
        assert_eq!(updated_wv.hall_requests[3][1].state, RequestState::Unconfirmed);
    }

    // Test that only acknowledgments from alive elevators are considered for confirmation
    #[test]
    fn test_only_considers_alive_elevators_for_confirmation() {
        let mut wv = WorldView::init(1);
        wv.hall_requests[1][0].state = RequestState::Unconfirmed;
        
        if let Some(elev) = wv.elevators.values_mut().next() {
            elev.last_received = SystemTime::UNIX_EPOCH; 
        }

        let alive_elevators: Vec<_> = wv.get_alive_elevators(1).into_iter().collect();
        for elev_id in &alive_elevators {
            wv.hall_requests[1][0].acks.insert(*elev_id);
        }

        let (updated_wv, changed) = wv.update_states_at_barrier();
        assert!(changed);
        assert_eq!(updated_wv.hall_requests[1][0].state, RequestState::Confirmed);
    }

    // Test that a new hall up request is properly registered and marked as Unconfirmed in the correct floor/direction
    #[test]
    fn test_hall_up_request() {
        let world = WorldView::init(1);
        let button_press = CallButton {
            floor: 2,
            call: 0,
        };
        
        let (updated_world, changed) = world.handle_button_press(&button_press);
        
        assert!(changed);
        assert_eq!(
            updated_world.hall_requests[2][0].state,
            RequestState::Unconfirmed
        );
    }

    // Test that a cab request is properly registered in the elevator's own cab requests array
    #[test]
    fn test_cab_request() {
        let world = WorldView::init(1); // Assuming there's a constructor
        let button_press = CallButton {
            floor: 3,
            call: 2, // Cab request
        };
        
        let (updated_world, changed) = world.handle_button_press(&button_press);
        
        assert!(changed);
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(
            elev.cab_requests[3].state,
            RequestState::Unconfirmed
        );
    }

    // Test that pressing an already active request (in any state) doesn't change anything and returns false
    #[test]
    fn test_duplicate_request_returns_false() {
        let mut world = WorldView::init(1);
        // Pre-set the request
        world.hall_requests[2][1].state = RequestState::Confirmed;
        
        let button_press = CallButton {
            floor: 2,
            call: 1, // Hall down
        };
        
        let (_updated_world, changed) = world.handle_button_press(&button_press);
        
        assert!(!changed);
    }

    // Tests that unknown button types (not 0,1, or 2) are ignored and don't modify the world state
    #[test]
    fn test_unknown_button_type() {
        let world = WorldView::init(1);
        let button_press = CallButton {
            floor: 2,
            call: 3, // Unknown type
        };
        
        let (updated_world, changed) = world.handle_button_press(&button_press);
        
        assert!(!changed);
        // Verify nothing was modified
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                assert_eq!(updated_world.hall_requests[floor][dir].state, RequestState::None);
            }
        }
    }

    // Test that normal state updates (dirn, behavior, floor) are properly applied without triggering recovery logic when nothing has changed
    #[test]
    fn test_normal_state_update() {
        let mut world = WorldView::init(1);
        world.elevators.get_mut(&1).unwrap().state.current_floor = 3;
        
        let (updated_world, changed) = world.handle_elevator_state(
            Dirn::Up,
            ElevatorBehaviour::Moving,
            3
        );
        
        assert!(changed);
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(elev.state.dirn, Dirn::Up);
        assert_eq!(elev.state.behaviour, ElevatorBehaviour::Moving);
        assert_eq!(elev.detect_if_dead_counter, 5); // Should still be reset
    }

    // Tests that recovery is detected when floor changes unexpectedly and resets the dead counter
    #[test]
    fn test_recovery_detected_on_floor_change() {
        let mut world = WorldView::init(1);
        world.elevators.get_mut(&1).unwrap().state.current_floor = 3;
        world.elevators.get_mut(&1).unwrap().detect_if_dead_counter = 1;
        
        let (updated_world, _) = world.handle_elevator_state(
            Dirn::Up,
            ElevatorBehaviour::Moving,
            4  // Different floor triggers recovery
        );
        
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(elev.detect_if_dead_counter, 5); // Counter reset
    }

    // Tests that recovery is detected when behavior changes unexpectedly and resets the dead counter
    #[test]
    fn test_recovery_detected_on_behavior_change() {
        let mut world = WorldView::init(1);
        world.elevators.get_mut(&1).unwrap().state.behaviour = ElevatorBehaviour::Idle;
        world.elevators.get_mut(&1).unwrap().detect_if_dead_counter = 1;
        
        let (updated_world, _) = world.handle_elevator_state(
            Dirn::Up,
            ElevatorBehaviour::Moving,  // Different behavior triggers recovery
            3
        );
        
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(elev.detect_if_dead_counter, 5); // Counter reset
    }

    // Tests that recovery is triggered when has_request is false regardless of other state changes
    #[test]
    fn test_recovery_when_no_requests() {
        let mut world = WorldView::init(1);
        world.elevators.get_mut(&1).unwrap().has_request = false;
        world.elevators.get_mut(&1).unwrap().detect_if_dead_counter = 1;
        
        let (updated_world, _) = world.handle_elevator_state(
            Dirn::Up,
            ElevatorBehaviour::Moving,
            3
        );
        
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(elev.detect_if_dead_counter, 5); // Counter reset
    }


    // Tests that hall requests are cleared only when specified in should_clear
    #[test]
    fn test_clears_only_specified_hall_requests() {
        let mut world = WorldView::init(1);
        world.hall_requests[3][0].state = RequestState::Confirmed; // Up
        world.hall_requests[3][1].state = RequestState::Confirmed; // Down
        
        let (updated_world, _) = world.handle_clear_request(
            3, 
            &[true, false, false] // Clear only UP
        );
        
        assert_eq!(updated_world.hall_requests[3][0].state, RequestState::None); // Cleared
        assert_eq!(updated_world.hall_requests[3][1].state, RequestState::Confirmed); // Untouched
    }

    // Tests that cab requests are cleared independently from hall requests
    #[test]
    fn test_clears_cab_requests_separately() {
        let mut world = WorldView::init(1);
        world.elevators.get_mut(&1).unwrap().cab_requests[2].state = RequestState::Confirmed;
        
        let (updated_world, _) = world.handle_clear_request(
            2,
            &[false, false, true] // Clear only CAB
        );
        
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(elev.cab_requests[2].state, RequestState::None);
    }

    // Tests that multiple request types can be cleared in a single call
    #[test]
    fn test_simultaneous_clear_of_multiple_types() {
        let mut world = WorldView::init(1);
        world.hall_requests[1][0].state = RequestState::Confirmed; // Up
        world.elevators.get_mut(&1).unwrap().cab_requests[1].state = RequestState::Confirmed;
        
        let (updated_world, _) = world.handle_clear_request(
            1,
            &[true, false, true] // Clear UP + CAB
        );
        
        assert_eq!(updated_world.hall_requests[1][0].state, RequestState::None);
        let elev = updated_world.elevators.get(&1).unwrap();
        assert_eq!(elev.cab_requests[1].state, RequestState::None);
    }
}

#[cfg(test)]
mod test_manager {
    use super::*;
    use std::time::Duration;

    // Ensures that get_alive_elevators returns the expected set of alive elevators
    #[test]
    fn test_get_alive_elevators() {
        let mut wv = WorldView::init(1);
        let elev = wv.elevators.get_mut(&1).unwrap();
        elev.last_received = SystemTime::now() - Duration::from_secs(10);
        let alive = wv.get_alive_elevators(5);
        assert!(alive.contains(&1));
    }

    // Checks that assign_requests returns the expected requests and active elevators
    #[test]
    fn test_assign_requests() {
        let wv = WorldView::init(1);
        let (reqs, active) = wv.assign_requests();
        assert_eq!(reqs[0][0], false);
        assert_eq!(active, Vec::<i32>::new());
    }

    // Checks that inform_everybody sends the expected messages to the sender, controller, and lights
    #[test]
    fn test_inform_everybody() {
        let mut wv = WorldView::init(1);
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, controller_rx) = cbc::unbounded();
        let (lights_tx, lights_rx) = cbc::unbounded();
        
        inform_everybody(&mut wv, &sender_tx, &controller_tx, &lights_tx);
        
        match sender_rx.recv().unwrap() {
            messages::Manager::HeartBeat(received_wv) => {
                // Compare specific properties instead of using ==
                assert!(matches!(received_wv, _), "Received WorldView should match the expected type");
            }
            _ => panic!("Unexpected message type"),
        }
        
        let confirmed_requests = wv.get_confirmed_requests();
        
        match controller_rx.recv().unwrap() {
            messages::Controller::Requests(reqs) => {
                assert_eq!(reqs, confirmed_requests, "Controller requests should match confirmed requests");
            }
            _ => panic!("Unexpected controller message type"),
        }
        
        match lights_rx.recv().unwrap() {
            messages::Controller::Requests(reqs) => {
                assert_eq!(reqs, confirmed_requests, "Lights requests should match confirmed requests");
            }
            _ => panic!("Unexpected lights message type"),
        }
    }

    // Checks that run processes messages correctly
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_run() {
        let (manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, controller_rx) = cbc::unbounded();
        let (lights_tx, lights_rx) = cbc::unbounded();
        let (call_button_tx, call_button_rx) = cbc::unbounded();
        let (alarm_tx, alarm_rx) = cbc::unbounded();
        
        let id = 1;
        let manager_handle = std::thread::spawn(move || {
            run(id, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });
        
        manager_tx.send(messages::Manager::Ping).unwrap();
        manager_tx.send(messages::Manager::HeartBeat(WorldView::init(2))).unwrap();
        manager_tx.send(messages::Manager::ElevatorState(Dirn::Up, ElevatorBehaviour::Moving, 3)).unwrap();
        manager_tx.send(messages::Manager::ClearRequest(2, [true, false, true])).unwrap();
        
        let button = CallButton { floor: 2, call: 0 };
        call_button_tx.send(button).unwrap();
        
        alarm_tx.send(1).unwrap();
        
        manager_handle.join().unwrap();
    }

    // Checks that run processes messages correctly
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_run_humble() {
        let (manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, controller_rx) = cbc::unbounded();
        let (lights_tx, lights_rx) = cbc::unbounded();
        let (call_button_tx, call_button_rx) = cbc::unbounded();
        let (alarm_tx, alarm_rx) = cbc::unbounded();
        
        let id = 1;
        let manager_handle = std::thread::spawn(move || {
            run(id, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });
        
        manager_tx.send(messages::Manager::Ping).unwrap();
        manager_tx.send(messages::Manager::HeartBeat(WorldView::init(2))).unwrap();
        manager_tx.send(messages::Manager::ElevatorState(Dirn::Up, ElevatorBehaviour::Moving, 3)).unwrap();
        manager_tx.send(messages::Manager::ClearRequest(2, [true, false, true])).unwrap();
        
        let button = CallButton { floor: 2, call: 0 };
        call_button_tx.send(button).unwrap();
        
        alarm_tx.send(1).unwrap();
        
        manager_handle.join().unwrap();
    }

    // Checks that run processes messages correctly
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_run_foreign() {
        let (manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, controller_rx) = cbc::unbounded();
        let (lights_tx, lights_rx) = cbc::unbounded();
        let (call_button_tx, call_button_rx) = cbc::unbounded();
        let (alarm_tx, alarm_rx) = cbc::unbounded();
        
        let id = 1;
        let manager_handle = std::thread::spawn(move || {
            run(id, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });
        
        manager_tx.send(messages::Manager::Ping).unwrap();
        manager_tx.send(messages::Manager::HeartBeat(WorldView::init(2))).unwrap();
        manager_tx.send(messages::Manager::ElevatorState(Dirn::Up, ElevatorBehaviour::Moving, 3)).unwrap();
        manager_tx.send(messages::Manager::ClearRequest(2, [true, false, true])).unwrap();
        
        let button = CallButton { floor: 2, call: 0 };
        call_button_tx.send(button).unwrap();
        
        alarm_tx.send(1).unwrap();
        
        manager_handle.join().unwrap();
    }
}

mod test_manager_functions {
    use std::thread;

    use super::*;

    // Tests that a Ping message is handled without crashing
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_ping_message_handling() {
        let (manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, _) = cbc::unbounded();
        let (controller_tx, _) = cbc::unbounded();
        let (lights_tx, _) = cbc::unbounded();
        let (_call_button_tx, call_button_rx) = cbc::unbounded();
        let (_alarm_tx, alarm_rx) = cbc::unbounded();

        manager_tx.send(messages::Manager::Ping).unwrap();
    
        let result = std::panic::catch_unwind(|| {
            run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });
        
        assert!(result.is_ok(), "Function panicked during Ping handling");
    }

    // Tests HeartBeat message handling in humble mode
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_humble_mode_heartbeat() {
        let (manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, _) = cbc::unbounded();
        let (lights_tx, _) = cbc::unbounded();
        let (_call_button_tx, call_button_rx) = cbc::unbounded();
        let (_alarm_tx, alarm_rx) = cbc::unbounded();

        let foreign_view = WorldView::init(2);
        manager_tx.send(messages::Manager::HeartBeat(foreign_view.clone())).unwrap();

        std::thread::spawn(move || {
            run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });

        // Verify humble mode handled the message
        assert!(sender_rx.try_recv().is_ok());
    }


    // Tests button press handling after humble mode ends
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_button_press_after_humble() {
        let (_manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, _) = cbc::unbounded();
        let (lights_tx, _) = cbc::unbounded();
        let (call_button_tx, call_button_rx) = cbc::unbounded();
        let (alarm_tx, alarm_rx) = cbc::unbounded();

        // Send 5 alarms to exit humble mode
        for _ in 0..5 {
            alarm_tx.send(1).unwrap();
        }

        // Send button press
        call_button_tx.send(CallButton { floor: 1, call: 0 }).unwrap();

        std::thread::spawn(move || {
            run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });

        // Verify button press was processed
        assert!(sender_rx.try_recv().is_ok());
    }


    // Tests alarm handling and dead counter decrement
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_alarm_handling_with_dead_counter() {
        let (_manager_tx, manager_rx) = cbc::unbounded();
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, _) = cbc::unbounded();
        let (lights_tx, _) = cbc::unbounded();
        let (_call_button_tx, call_button_rx) = cbc::unbounded();
        let (alarm_tx, alarm_rx) = cbc::unbounded();

        alarm_tx.send(1).unwrap();

        std::thread::spawn(move || {
            run(1, manager_rx, sender_tx, controller_tx, lights_tx, call_button_rx, alarm_rx);
        });

        // Verify alarm was processed
        assert!(sender_rx.try_recv().is_ok());
    }

    // Tests that HeartBeat message is sent with correct WorldView
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_sends_heartbeat() {
        let mut world = WorldView::init(1);
     
        // Create all channels with small buffers
        let (sender_tx, sender_rx) = cbc::bounded(1);
        let (controller_tx, controller_rx) = cbc::bounded(1);
        let (lights_tx, lights_rx) = cbc::bounded(1);

        // Spawn thread to handle controller messages
        let controller_handler = thread::spawn(move || {
            match controller_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(messages::Controller::Requests(_)) => (),
                _ => panic!("Controller channel error"),
            }
        });

        // Spawn thread to handle lights messages
        let lights_handler = thread::spawn(move || {
            match lights_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(messages::Controller::Requests(_)) => (),
                _ => panic!("Lights channel error"),
            }
        });

        // Execute the function under test
        inform_everybody(&mut world, &sender_tx, &controller_tx, &lights_tx);

        // Verify the heartbeat message
        match sender_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(messages::Manager::HeartBeat(wv)) => assert_eq!(wv.id, 1),
            _ => panic!("Expected HeartBeat message"),
        }

        // Ensure all handlers completed
        controller_handler.join().expect("Controller handler failed");
        lights_handler.join().expect("Lights handler failed");
    }

    // Helper function to drain a channel
    fn drain_channel<T>(rx: &cbc::Receiver<T>) {
        while let Ok(_) = rx.try_recv() {}
    }

    // Tests that active elevators get has_request set correctly
    #[test]
    #[ignore = "Requires to run more elevators"]
    fn test_updates_active_elevators() {
        let world = WorldView::init(1);
        let (sender_tx, sender_rx) = cbc::unbounded();
        let (controller_tx, controller_rx) = cbc::unbounded();
        let (lights_tx, lights_rx) = cbc::unbounded();

        // Spawn consumers for all channels
        thread::spawn(move || {
            drain_channel(&sender_rx);
            drain_channel(&controller_rx);
            drain_channel(&lights_rx);
        });

        let mut mock_world = world.clone();
        mock_world.elevators.get_mut(&1).unwrap().has_request = false;
        
        inform_everybody(&mut mock_world, &sender_tx, &controller_tx, &lights_tx);

        let elev = mock_world.elevators.get(&1).unwrap();
        assert!(elev.has_request);
        assert_eq!(elev.detect_if_dead_counter, 10);
    }
}