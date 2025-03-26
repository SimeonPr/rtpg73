use crate::config;
use crate::cost;
use crate::models::{Elevator, Request, RequestState};
use crate::models::{Dirn, ElevatorBehaviour};
use driver_rust::elevio::poll::CallButton;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};
use log::{info};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldView {
    id: u8,
    elevators: HashMap<u8, Elevator>,
    hall_requests: [[Request; 2]; config::FLOOR_COUNT],
}

impl WorldView {
    pub fn init(id: u8) -> Self {
        let mut elevators = HashMap::new();
        elevators.insert(id, Elevator::new());
        let mut hall_requests: [[Request; 2]; config::FLOOR_COUNT] = Default::default();
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                hall_requests[floor][dir] = Request::new(id);
            }
        }

        WorldView {
            id,
            elevators,
            hall_requests,
        }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn elevators(&self) -> &HashMap<u8, Elevator> {
        &self.elevators
    }

    pub fn elevators_mut(&mut self) -> &mut HashMap<u8, Elevator> {
        &mut self.elevators
    }

    pub fn hall_requests(&self) -> &[[Request; 2]; config::FLOOR_COUNT] {
        &self.hall_requests
    }

    pub fn hall_requests_mut(&mut self) -> &mut [[Request; 2]; config::FLOOR_COUNT] {
        &mut self.hall_requests
    }

    pub fn compare_world_views(&self, other: &WorldView) {
        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if self.hall_requests[floor][dir].state() != other.hall_requests[floor][dir].state() {
                    info!(
                        "HallRequestState(floor: {}, dir: {}): {:?} -> {:?}",
                        floor,
                        dir,
                        self.hall_requests[floor][dir].state(),
                        other.hall_requests[floor][dir].state()
                    );
                }
            }
        }

        for key in other.elevators().keys() {
            if !self.elevators.contains_key(key) {
                info!("NewElevator(id: {})", key);
            }
        }

        for (key, elev) in self.elevators.iter() {
            if let Some(other_elev) = other.elevators().get(key) {
                for floor in 0..config::FLOOR_COUNT {
                    if elev.cab_requests()[floor].state() != other_elev.cab_requests()[floor].state() {
                        info!(
                            "CabRequestState(id: {}, floor: {}): {:?} -> {:?}",
                            key,
                            floor,
                            elev.cab_requests()[floor].state(),
                            other_elev.cab_requests()[floor].state()
                        );
                    }
                }

                if elev.state.dirn != other_elev.state.dirn {
                    info!("Dirn(id: {}): {:?} -> {:?}", key, elev.state.dirn, other_elev.state.dirn);
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

    pub fn handle_humbly(&self, foreign: WorldView) -> Self {
        let mut clone = foreign.clone();
        let id = self.id;
        if let Some(own_elev) = self.elevators.get(&id) {
            clone.elevators.insert(id, own_elev.clone());
        }
        clone.id = id;
        clone
    }

    pub fn handle_foreign_world_view(&self, foreign: WorldView) -> (Self, bool) {
        let mut clone = self.clone();
        let mut updated = false;

        let current_time = SystemTime::now();
        let foreign_id = foreign.id();
        let foreign_elevators = foreign.elevators();

        for (id, elev) in foreign_elevators {
            if !clone.elevators.contains_key(id) {
                info!("NewElevator(id: {})", id);
                clone.elevators.insert(*id, elev.clone());
            }
        }

        if let Some(elev) = foreign_elevators.get(&foreign_id) {
            if let Some(own) = clone.elevators.get_mut(&foreign_id) {
                if own.state.current_floor != elev.state.current_floor
                    || own.state.behaviour != elev.state.behaviour
                {
                    if !own.is_working() {
                        info!("Foreign Elevator {} recovered", foreign_id);
                    }
                    own.set_last_moved(SystemTime::now());
                    own.set_is_working(true);
                }
                own.set_last_received(current_time);
                own.state = elev.state;
            }
        }

        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if clone.hall_requests[floor][dir].merge(&foreign.hall_requests[floor][dir], clone.id) {
                    updated = true;
                }
            }
        }

        for (id, foreign_elev) in foreign_elevators {
            if let Some(own_elev) = clone.elevators.get_mut(id) {
                for floor in 0..config::FLOOR_COUNT {
                    if own_elev.cab_requests_mut()[floor].merge(
                        &foreign_elev.cab_requests()[floor],
                        clone.id,
                    ) {
                        updated = true;
                    }
                }
            }
        }

        let (clone, barrier_updated) = clone.update_states_at_barrier();
        (clone, updated || barrier_updated)
    }

    pub fn update_states_at_barrier(&self) -> (Self, bool) {
        let mut clone = self.clone();
        let mut updated = false;
        let alive = clone.get_alive_elevators(1);

        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if clone.hall_requests[floor][dir].state() == RequestState::Unconfirmed
                    && alive.is_subset(clone.hall_requests[floor][dir].acks())
                {
                    clone.hall_requests[floor][dir].set_to(RequestState::Confirmed, self.id);
                    updated = true;
                }
            }
        }

        for elev in clone.elevators.values_mut() {
            for floor in 0..config::FLOOR_COUNT {
                if elev.cab_requests()[floor].state() == RequestState::Unconfirmed
                    && alive.is_subset(elev.cab_requests()[floor].acks())
                {
                    elev.cab_requests_mut()[floor].set_to(RequestState::Confirmed, self.id);
                    updated = true;
                }
            }
        }

        (clone, updated)
    }

    pub fn handle_button_press(&self, button: &CallButton) -> (Self, bool) {
        let mut clone = self.clone();
        let mut updated = false;

        match button.call {
            0 | 1 => {
                let req = &mut clone.hall_requests[button.floor as usize][button.call as usize];
                if req.state() == RequestState::None {
                    req.set_to(RequestState::Unconfirmed, clone.id);
                    updated = true;
                }
            }
            2 => {
                let elev = clone.elevators.get_mut(&clone.id).unwrap();
                let req = &mut elev.cab_requests_mut()[button.floor as usize];
                if req.state() == RequestState::None {
                    req.set_to(RequestState::Unconfirmed, clone.id);
                    updated = true;
                }
            }
            _ => {}
        }

        (clone, updated)
    }

    pub fn handle_elevator_state(
        &self,
        dirn: Dirn,
        behaviour: ElevatorBehaviour,
        floor: i8,
    ) -> (Self, bool) {
        let mut clone = self.clone();
        let elev = clone.elevators.get_mut(&clone.id).unwrap();

        if elev.state.current_floor != floor || elev.state.behaviour != behaviour {
            if !elev.is_working() {
                info!("Own elevator recovered");
            }
            elev.set_last_moved(SystemTime::now());
            elev.set_is_working(true);
        }

        elev.state.dirn = dirn;
        elev.state.behaviour = behaviour;
        elev.state.current_floor = floor;
        (clone, true)
    }

    pub fn handle_clear_request(&self, floor: usize, should_clear: &[bool; 3]) -> (Self, bool) {
        let mut clone = self.clone();
        let elev = clone.elevators.get_mut(&clone.id).unwrap();

        for i in 0..2 {
            if should_clear[i] {
                clone.hall_requests[floor][i].set_to(RequestState::None, clone.id);
            }
        }

        if should_clear[2] {
            elev.cab_requests_mut()[floor].set_to(RequestState::None, clone.id);
        }

        (clone, true)
    }

    pub fn get_alive_elevators(&self, timeout: u64) -> HashSet<u8> {
        let mut alive = HashSet::new();
        for (&id, elev) in self.elevators.iter() {
            if id != self.id
                && elev.last_received().elapsed().unwrap_or(Duration::MAX)
                    > Duration::from_secs(timeout)
                || !elev.is_working()
            {
                continue;
            }
            alive.insert(id);
        }
        alive
    }

    pub fn assign_requests(&self) -> ([[bool; config::CALL_COUNT]; config::FLOOR_COUNT], Vec<i32>) {
        match cost::elevator_algorithm(self) {
            Some((mut requests, active)) => {
                let confirmed = self.get_confirmed_requests();
                for floor in 0..config::FLOOR_COUNT {
                    requests[floor][2] = confirmed[floor][2];
                }
                (requests, active)
            }
            None => {
                (self.get_confirmed_requests(), vec![])
            }
        }
    }

    pub fn get_confirmed_requests(&self) -> [[bool; config::CALL_COUNT]; config::FLOOR_COUNT] {
        let mut result = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];
        let elev = self.elevators.get(&self.id).unwrap();

        for floor in 0..config::FLOOR_COUNT {
            for dir in 0..2 {
                if self.hall_requests[floor][dir].state() == RequestState::Confirmed {
                    result[floor][dir] = true;
                }
            }
            if elev.cab_requests()[floor].state() == RequestState::Confirmed {
                result[floor][2] = true;
            }
        }

        result
    }
}
