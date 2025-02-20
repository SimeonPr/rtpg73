use driver_rust::elevio::elev::Elevator;
use log::trace;
use serde::{Serialize, Deserialize};

use std::thread;
use std::time::Duration;
use crossbeam_channel as cbc;
use crate::config::FLOOR_COUNT;

const CALL_COUNT: usize = 3;
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ElevatorBehaviour {
    Idle,
    DoorOpen,
    Moving
}
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Dirn {
    Down = -1,
    Stop = 0,
    Up = 1
}
#[derive(Debug)]
enum Button {
    HallUp,
    HallDown,
    Cab
}
#[derive(Debug)]
pub struct ElevatorState {
    timer_tx: cbc::Sender<bool>,
    no_of_timer_threads: u8,
    floor: i8,
    dirn: Dirn,
    requests: [[i32; CALL_COUNT]; FLOOR_COUNT],
    behaviour: ElevatorBehaviour,
    door_open_duration: u64,
    connection: Elevator,
    obstruction: bool
}
#[derive(Debug)]
struct DirectionBehaviourPair {
    dirn: Dirn,
    behavior: ElevatorBehaviour
}
impl ElevatorState {
    
    pub fn init_elevator(elevator_connection: Elevator, timer_tx: cbc::Sender<bool>) -> ElevatorState {
        trace!("init_elevator");
        ElevatorState {
            timer_tx,
            no_of_timer_threads: 0,
            floor: -1,
            dirn: Dirn::Stop,
            requests: [[0;CALL_COUNT]; FLOOR_COUNT],
            behaviour: ElevatorBehaviour::Idle,
            door_open_duration: 3,
            connection: elevator_connection,
            obstruction: false
        }
    }
    
    pub fn fsm_on_init_between_floors(&mut self) {
        trace!("fsm_on_init_between_floors");
        //motor direction
        self.connection.motor_direction(Dirn::Down as u8);
        self.dirn = Dirn::Down;
        self.behaviour = ElevatorBehaviour::Moving;
    }
    
    pub fn fsm_on_request_button_press(&mut self, floor: i8, call: u8) {
        trace!("fsm_on_request_button_press");
        match self.behaviour {
            ElevatorBehaviour::DoorOpen => {
                if self.requests_should_clear_immediately(floor, call) {
                    self.start_time_out_thread();
                } else {
                    self.requests[floor as usize][call as usize] = 1;
                }
            },
            ElevatorBehaviour::Moving => {
                self.requests[floor as usize][call as usize] = 1;
            },
            ElevatorBehaviour::Idle => {
                self.requests[floor as usize][call as usize] = 1;
                let direction_behavior_pair = self.requests_choose_direction();
                self.dirn = direction_behavior_pair.dirn;
                self.behaviour = direction_behavior_pair.behavior;
                match self.behaviour {
                    ElevatorBehaviour::Idle => {},
                    ElevatorBehaviour::DoorOpen => {
                        self.connection.door_light(true);
                        self.start_time_out_thread();
                        self.requests_clear_at_current_floor();
                    },
                    ElevatorBehaviour::Moving => {
                        self.connection.motor_direction(self.dirn as u8);
                    }
                };
            }
        };
        
        self.set_all_lights();
    }

    pub fn fsm_on_door_time_out(&mut self) {
        trace!("fsm_on_door_time_out");
        self.no_of_timer_threads -= 1;
        if self.no_of_timer_threads > 0 {return;}
        if self.obstruction  {
            self.start_time_out_thread();
            return;
        }
        match self.behaviour {
            ElevatorBehaviour::DoorOpen => {
                let pair: DirectionBehaviourPair = self.requests_choose_direction();
                self.dirn = pair.dirn;
                self.behaviour = pair.behavior;

                match self.behaviour {
                    ElevatorBehaviour::DoorOpen => {
                        self.start_time_out_thread();
                        self.requests_clear_at_current_floor();
                        self.set_all_lights();
                    },
                    ElevatorBehaviour::Moving | ElevatorBehaviour::Idle => {
                        self.connection.door_light(false);
                        self.connection.motor_direction(self.dirn as u8);
                    }
                }
            },
            _ => {}
        }
    }
    pub fn fsm_on_obstruction(&mut self, val: bool) {
        trace!("fsm_on_obstruction");
        self.obstruction = val;
    }
    pub fn fsm_on_floor_arrival(&mut self, floor: i8) {
        trace!("fsm_on_floor_arrival");
        self.floor = floor;
        self.connection.floor_indicator(self.floor as u8);

        match self.behaviour {
            ElevatorBehaviour::Moving => {
                if self.requests_should_stop() {
                    self.connection.motor_direction(Dirn::Stop as u8);
                    self.connection.door_light(true);
                    self.requests_clear_at_current_floor();
                    self.start_time_out_thread();
                    self.set_all_lights();
                    self.behaviour = ElevatorBehaviour::DoorOpen;
                }
            }
            _ => {},
        };
    }

    pub fn fsm_on_stop_button_press(&mut self){}

    fn start_time_out_thread(&mut self) {
        trace!("sleep");
        self.no_of_timer_threads += 1;
        let timer_tx_clone = self.timer_tx.clone();
        let duration = self.door_open_duration;
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(duration));
            timer_tx_clone.send(true).unwrap();
        });
    }
    fn requests_should_clear_immediately(&mut self, floor: i8, _call: u8) -> bool {
        trace!("request_should_clear_immediately");
         self.floor == floor
    }
    
    fn set_all_lights(&self) {
        trace!("set_all_lights");
        for f in 0..FLOOR_COUNT {
            for b in 0..CALL_COUNT {
                self.connection.call_button_light(f as u8, b as u8, self.requests[f as usize][b as usize] == 1);
            }
        }
    }
    
    fn requests_choose_direction(&mut self) -> DirectionBehaviourPair {
        trace!("requests_choose_direction");
        match self.dirn {
            Dirn::Up => {
                if self.requests_above() {
                    DirectionBehaviourPair {dirn: Dirn::Up, behavior: ElevatorBehaviour::Moving}
                } else if self.requests_here() {
                    DirectionBehaviourPair {dirn: Dirn::Down, behavior: ElevatorBehaviour::DoorOpen}
                } else if self.requests_below() {
                    DirectionBehaviourPair {dirn: Dirn::Down, behavior: ElevatorBehaviour::Moving}
                } else {
                    DirectionBehaviourPair {dirn: Dirn::Stop, behavior: ElevatorBehaviour::Idle}
                }
            },
            Dirn::Down => {
                if self.requests_below() {
                    DirectionBehaviourPair {dirn: Dirn::Down, behavior: ElevatorBehaviour::Moving}
                } else if self.requests_here() {
                    DirectionBehaviourPair {dirn: Dirn::Up, behavior: ElevatorBehaviour::DoorOpen}
                } else if self.requests_above() {
                    DirectionBehaviourPair {dirn: Dirn::Up, behavior: ElevatorBehaviour::Moving}
                } else {
                    DirectionBehaviourPair {dirn: Dirn::Stop, behavior: ElevatorBehaviour::Idle}
                }
            },
            Dirn::Stop => {
                if self.requests_here() {
                    DirectionBehaviourPair {dirn: Dirn::Stop, behavior: ElevatorBehaviour::DoorOpen}
                } else if self.requests_above() {
                    DirectionBehaviourPair {dirn: Dirn::Up, behavior: ElevatorBehaviour::Moving}
                } else if self.requests_below() {
                    DirectionBehaviourPair {dirn: Dirn::Down, behavior: ElevatorBehaviour::Moving}
                } else {
                    DirectionBehaviourPair {dirn: Dirn::Stop, behavior: ElevatorBehaviour::Idle}
                }
            }
        }
    }
    
    fn requests_clear_at_current_floor(&mut self) {
        trace!("requests_clear_at_current_floor");
        //for b in 0..CALL_COUNT {
          //  self.requests[self.floor as usize][b as usize] = 0;
        //}
        self.requests[self.floor as usize][Button::Cab as usize] = 0;
        match self.dirn {
            Dirn::Up => {
                if !self.requests_above() && (self.requests[self.floor as usize][Button::HallUp as usize] == 0) {
                    self.requests[self.floor as usize][Button::HallDown as usize] = 0;
                }
                self.requests[self.floor as usize][Button::HallUp as usize] = 0;
            },
            Dirn::Down => {
                if !self.requests_below() && (self.requests[self.floor as usize][Button::HallDown as usize] == 0) {
                    self.requests[self.floor as usize][Button::HallUp as usize] = 0;
                }
                self.requests[self.floor as usize][Button::HallDown as usize] = 0;
            },
            Dirn::Stop => {
               self.requests[self.floor as usize][Button::HallUp as usize] = 0;
               self.requests[self.floor as usize][Button::HallDown as usize] = 0;
            }
        }
    }
    
    fn requests_here(&self) -> bool {
        trace!("requests_here");
        for b in 0..CALL_COUNT {
            if self.requests[self.floor as usize][b as usize] == 1 {
                return true;
            }
        }
        return false;
    }
    
    fn requests_below(&self) -> bool {
        trace!("requests_below");
        for f in 0..self.floor {
            for b in 0..CALL_COUNT {
                if self.requests[f as usize][b as usize] == 1 {
                    return true;
                }
            }
        }
        return false;
    }
    
    fn requests_above(&self) -> bool {
        trace!("requests_above");
        for f in ((self.floor+1) as usize)..FLOOR_COUNT {
            for b in 0..CALL_COUNT {
                if self.requests[f as usize][b as usize] == 1 {
                    return true;
                }
            }
        }
        return false;
    }
    
    fn requests_should_stop(&self) -> bool {
        trace!("requests_should_stop");
        match self.dirn {
            Dirn::Down => {
                self.requests[self.floor as usize][Button::HallDown as usize] == 1||
                    self.requests[self.floor as usize][Button::Cab as usize] == 1||
                    !self.requests_below()
            },
            Dirn::Up => {
                self.requests[self.floor as usize][Button::HallUp as usize] == 1||
                    self.requests[self.floor as usize][Button::Cab as usize] == 1||
                    !self.requests_above()                
            },
            Dirn::Stop => {true}
        }
    }
    
}
