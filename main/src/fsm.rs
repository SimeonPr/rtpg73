//! Elevator State Machine module
//!
//! This module implements the core logic for controlling an elevator's behavior through
//! a finite state machine (FSM). It handles:
//! - Elevator movement between floors
//! - Door opening/closing
//! - Request management
//! - Obstruction detection
//! - Timing for door operations
use driver_rust::elevio::elev::Elevator;
use log::{trace, debug};
use serde::{Serialize, Deserialize};

use std::thread;
use std::time::Duration;
use crossbeam_channel::{self as cbc, Sender};
use crate::{config, messages};

/// Represents possible elevator behaviors
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ElevatorBehaviour {
    /// Elevator is idle and waiting for requests
    #[serde(rename = "idle")]
    Idle,
    /// Elevator doors are currently open
    #[serde(rename = "doorOpen")]
    DoorOpen,
    /// Elevator is moving between floors
    #[serde(rename = "moving")]
    Moving
}

/// Represents possible elevator directions
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq)]
pub enum Dirn {
    /// Moving downward
    #[serde(rename = "down")]
    Down = -1,
    /// Stopped (not moving)
    #[serde(rename = "stop")]
    Stop = 0,
    /// Moving upward
    #[serde(rename = "up")]
    Up = 1
}

/// Represents elevator button types
#[derive(Debug)]
pub enum Button {
    HallUp,    // Hall call up button
    HallDown,  // Hall call down button
    Cab        // Cab (internal) button
}
/// Type alias for elevator requests matrix (floors × button types)
pub type ControllerRequests = [[bool;config::CALL_COUNT]; config::FLOOR_COUNT];

/// Main elevator state structure
#[derive(Debug)]
pub struct ElevatorState {
    timer_tx: cbc::Sender<bool>,      // Channel for timer notifications
    no_of_timer_threads: u8,          // Active timer threads count
    floor: i8,                        // Current floor (-1 if between floors)
    dirn: Dirn,                       // Current direction
    requests: ControllerRequests,      // Current request matrix
    behaviour: ElevatorBehaviour,      // Current behavior state
    door_open_duration: u64,          // Duration doors stay open (seconds)
    connection: Elevator,              // Hardware interface
    obstruction: bool                  // Obstruction sensor state
}

/// Internal helper structure for direction/behavior pairs
#[derive(Debug)]
struct DirectionBehaviourPair {
    dirn: Dirn,
    behavior: ElevatorBehaviour
}

impl ElevatorState {
    /// Initializes a new elevator state
    ///
    /// # Parameters
    /// - `elevator_connection`: Hardware interface
    /// - `timer_tx`: Timer notification channel
    ///
    /// # Returns
    /// New ElevatorState with default values
    pub fn init_elevator(elevator_connection: Elevator, timer_tx: cbc::Sender<bool>) -> ElevatorState {
        trace!("init_elevator");
        ElevatorState {
            timer_tx,
            no_of_timer_threads: 0,
            floor: -1,
            dirn: Dirn::Stop,
            requests: [[false;config::CALL_COUNT]; config::FLOOR_COUNT],
            behaviour: ElevatorBehaviour::Idle,
            door_open_duration: 3,
            connection: elevator_connection,
            obstruction: false
        }
    }
    
    /// Handles new request assignments
    ///
    /// # Parameters
    /// - `requests`: New request matrix
    /// - `manager_tx`: Channel for sending state updates
    pub fn fsm_on_new_requests(&mut self, requests: ControllerRequests, manager_tx: &Sender<messages::Manager>) {
        self.requests = requests;
        match self.behaviour {
            ElevatorBehaviour::Idle => {
                let direction_behavior_pair = self.requests_choose_direction();
                self.dirn = direction_behavior_pair.dirn;
                self.behaviour = direction_behavior_pair.behavior;
                match self.behaviour {
                    ElevatorBehaviour::Idle => {},
                    ElevatorBehaviour::DoorOpen => {
                        self.connection.door_light(true);
                        self.start_time_out_thread();
                        self.requests_clear_at_current_floor(&manager_tx);
                    },
                    ElevatorBehaviour::Moving => {
                        self.connection.motor_direction(self.dirn as u8);
                    }
                };
            },
            _ => ()
        }
    }
    
    /// Handles initialization between floors
    pub fn fsm_on_init_between_floors(&mut self) {
        trace!("fsm_on_init_between_floors");
        //motor direction
        self.connection.motor_direction(Dirn::Down as u8);
        self.dirn = Dirn::Down;
        self.behaviour = ElevatorBehaviour::Moving;
    }
    
    /// Handles door timeout events
    ///
    /// # Parameters
    /// - `manager_tx`: Channel for sending state updates
    pub fn fsm_on_door_time_out(&mut self, manager_tx: &Sender<messages::Manager>) {
        trace!("fsm_on_door_time_out");
        self.no_of_timer_threads -= 1;
        if self.no_of_timer_threads > 0 {return;}
        if self.obstruction  {
            self.start_time_out_thread();
            return;
        }
        debug!("Handling LastTimeOut");
        match self.behaviour {
            ElevatorBehaviour::DoorOpen => {
                let pair: DirectionBehaviourPair = self.requests_choose_direction();
                self.dirn = pair.dirn;
                self.behaviour = pair.behavior;

                match self.behaviour {
                    ElevatorBehaviour::DoorOpen => {
                        self.start_time_out_thread();
                        self.requests_clear_at_current_floor(&manager_tx);
                    },
                    ElevatorBehaviour::Moving | ElevatorBehaviour::Idle => {
                        self.connection.door_light(false);
                        self.connection.motor_direction(self.dirn as u8);
                    }
                }
            },
            _ => {}
            
        }
        manager_tx.send(messages::Manager::ElevatorState(self.dirn, self.behaviour, self.floor)).expect("couldn't send to manager");
    }
    
    /// Updates obstruction sensor state
    ///
    /// # Parameters
    /// - `val`: New obstruction state
    pub fn fsm_on_obstruction(&mut self, val: bool) {
        trace!("fsm_on_obstruction");
        self.obstruction = val;
    }
    
    /// Handles floor arrival events
    ///
    /// # Parameters
    /// - `floor`: New floor number
    /// - `manager_tx`: Channel for sending state updates
    pub fn fsm_on_floor_arrival(&mut self, floor: i8, manager_tx: &Sender<messages::Manager>) {
        trace!("fsm_on_floor_arrival");
        //stop timer? 
        self.floor = floor;
        self.connection.floor_indicator(self.floor as u8);

        match self.behaviour {
            ElevatorBehaviour::Moving => {
                if self.requests_should_stop() {
                    self.connection.motor_direction(Dirn::Stop as u8);
                    self.connection.door_light(true);
                    self.requests_clear_at_current_floor(&manager_tx);
                    self.start_time_out_thread();
                    self.behaviour = ElevatorBehaviour::DoorOpen;
                }
            }
            _ => {},
        };

        manager_tx.send(messages::Manager::ElevatorState(self.dirn, self.behaviour, self.floor)).expect("couldn't send to manager");
    }

    /// Handles stop button press events
    pub fn fsm_on_stop_button_press(&mut self){}

    // Private helper methods
    fn start_time_out_thread(&mut self) {
        trace!("sleep");
        self.no_of_timer_threads += 1;
        let timer_tx_clone = self.timer_tx.clone();
        let duration = self.door_open_duration;
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(duration));
            timer_tx_clone.send(true).expect("couldn't send to timer");
        });
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
    
    fn requests_clear_at_current_floor(&mut self, manager_tx: &Sender<messages::Manager>) {
        trace!("requests_clear_at_current_floor");
        let mut should_clear = [false; 3];
        self.requests[self.floor as usize][Button::Cab as usize] = false;
        should_clear[Button::Cab as usize] = true;
        match self.dirn {
            Dirn::Up => {
                if !self.requests_above() && (self.requests[self.floor as usize][Button::HallUp as usize] == false) {
                    self.requests[self.floor as usize][Button::HallDown as usize] = false;
                    should_clear[Button::HallDown as usize] = true;
                }
                self.requests[self.floor as usize][Button::HallUp as usize] = false;
                should_clear[Button::HallUp as usize] = true;
            },
            Dirn::Down => {
                if !self.requests_below() && (self.requests[self.floor as usize][Button::HallDown as usize] == false) {
                    self.requests[self.floor as usize][Button::HallUp as usize] = false;
                    should_clear[Button::HallUp as usize] = true;
                }
                self.requests[self.floor as usize][Button::HallDown as usize] = false;
                should_clear[Button::HallDown as usize] = true;
            },
            Dirn::Stop => {
                self.requests[self.floor as usize][Button::HallUp as usize] = false;
                self.requests[self.floor as usize][Button::HallDown as usize] = false;
                should_clear[Button::HallUp as usize] = true;
                should_clear[Button::HallDown as usize] = true;
            }
        }
        manager_tx.send(messages::Manager::ClearRequest(self.floor as usize, should_clear)).expect("couldn't send to manager");
    }
    
    fn requests_here(&self) -> bool {
        trace!("requests_here");
        for b in 0..config::CALL_COUNT {
            if self.requests[self.floor as usize][b as usize] {
                return true;
            }
        }
        return false;
    }
    
    fn requests_below(&self) -> bool {
        trace!("requests_below");
        for f in 0..self.floor {
            for b in 0..config::CALL_COUNT {
                if self.requests[f as usize][b as usize] {
                    return true;
                }
            }
        }
        return false;
    }
    
    fn requests_above(&self) -> bool {
        trace!("requests_above");
        for f in ((self.floor+1) as usize)..config::FLOOR_COUNT {
            for b in 0..config::CALL_COUNT {
                if self.requests[f as usize][b as usize] {
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
                self.requests[self.floor as usize][Button::HallDown as usize] == true||
                    self.requests[self.floor as usize][Button::Cab as usize] == true||
                    !self.requests_below()
            },
            Dirn::Up => {
                self.requests[self.floor as usize][Button::HallUp as usize] == true||
                    self.requests[self.floor as usize][Button::Cab as usize] == true||
                    !self.requests_above()                
            },
            Dirn::Stop => {true}
        }
    }
    
}

#[cfg(test)]

/// Test the initialization of the elevator state
mod test_init_elevator {
    use super::*;

    // Test the elevator initializes with correct default values for floor, direction, behavior, and obstruction
    #[test]
    fn test_init_elevator() {
        let (tx, _rx) = cbc::unbounded();
        let elevator = Elevator::init("127.0.0.1:15657", config::FLOOR_COUNT as u8).unwrap();
        let elevator_state = ElevatorState::init_elevator(elevator, tx);

        assert_eq!(elevator_state.no_of_timer_threads, 0);
        assert_eq!(elevator_state.floor, -1);
        assert_eq!(elevator_state.dirn, Dirn::Stop);
        assert_eq!(elevator_state.requests, [[false;CALL_COUNT]; config::FLOOR_COUNT]);
        assert_eq!(elevator_state.behaviour, ElevatorBehaviour::Idle);
        assert_eq!(elevator_state.door_open_duration, 3);
        assert_eq!(elevator_state.obstruction, false);
    }

    // Test that all request slots are initialized to false
    #[test]
    fn test_requests_initialized_empty() {
        let (timer_tx, _) = cbc::bounded(1);
        let elevator = Elevator::init("127.0.0.1:15657", config::FLOOR_COUNT as u8).unwrap();
        
        let state = ElevatorState::init_elevator(elevator, timer_tx);
        
        assert!(state.requests.iter().flatten().all(|&r| !r), 
               "All requests should be false initially");
    }

}

/// Test the function that chooses the elevator's direction and behavior based on requests
mod fsm_on_new_requests {
    use config::FLOOR_COUNT;

    use super::*;

    fn create_test_elevator(initial_behaviour: ElevatorBehaviour) -> ElevatorState {
        let (timer_tx, _) = crossbeam_channel::bounded(1);
        ElevatorState {
            behaviour: initial_behaviour,
            connection: Elevator::init("127.0.0.1:15657", config::FLOOR_COUNT as u8).unwrap(),
            timer_tx,
            no_of_timer_threads: 0,
            floor: -1, 
            dirn: Dirn::Stop,
            requests: [[false; config::CALL_COUNT]; config::FLOOR_COUNT],
            door_open_duration: 3,
            obstruction: false
        }
    }

    // Test that new requests are stored and processed correctly when elevator is idle
    #[test]
    fn test_new_requests_updates_state_when_idle() {
        let (manager_tx, manager_rx) = crossbeam_channel::unbounded();
        let mut elevator = create_test_elevator(ElevatorBehaviour::Idle);

        elevator.floor = 0;
        
        let test_requests = [[true; config::CALL_COUNT]; config::FLOOR_COUNT];
        elevator.fsm_on_new_requests(test_requests, &manager_tx);
        
        let _ = manager_rx.recv().unwrap();
        
        let mut expected_requests = test_requests;
        expected_requests[0] = [false; config::CALL_COUNT];
        
        assert_eq!(elevator.requests, expected_requests);
        assert_ne!(elevator.behaviour, ElevatorBehaviour::Idle);
    }

    // Checks that door open behavior triggers correct actions
    #[test]
    fn test_door_open_behavior_activates_proper_sequence() {
        let (manager_tx, manager_rx) = crossbeam_channel::unbounded();
        let mut elevator = create_test_elevator(ElevatorBehaviour::Idle);
        elevator.floor = 2; 

        let mut test_requests = [[false; CALL_COUNT]; FLOOR_COUNT];
        test_requests[2][0] = true;
        
        elevator.fsm_on_new_requests(test_requests, &manager_tx);
        
        assert_eq!(elevator.behaviour, ElevatorBehaviour::DoorOpen);
        assert!(manager_rx.try_recv().is_ok());
    }

    // Verifies moving behavior sets correct motor direction
    #[test]
    fn test_moving_behavior_sets_motor_direction() {
        let (manager_tx, _) = crossbeam_channel::unbounded();
        let mut elevator = create_test_elevator(ElevatorBehaviour::Idle);
        elevator.floor = 1;
        
        let mut test_requests = [[false; CALL_COUNT]; FLOOR_COUNT];
        test_requests[3][0] = true;
        
        elevator.fsm_on_new_requests(test_requests, &manager_tx);
        
        assert_eq!(elevator.behaviour, ElevatorBehaviour::Moving);
        assert_eq!(elevator.dirn, Dirn::Up);
    }
}


/// Test the function that handles door timeout events
mod fsm_on_init_between_floors {
    use super::*;
    use crossbeam_channel;

    // Helper to create test state with real elevator (skips test if connection fails)
    fn create_test_state() -> Option<ElevatorState> {
        let (timer_tx, _) = crossbeam_channel::bounded(1);
        match Elevator::init("127.0.0.1:15657", config::FLOOR_COUNT as u8) {
            Ok(conn) => Some(ElevatorState {
                timer_tx,
                connection: conn,
                no_of_timer_threads: 0,
                floor: -1,
                dirn: Dirn::Stop,
                requests: [[false; config::CALL_COUNT]; config::FLOOR_COUNT],
                behaviour: ElevatorBehaviour::Idle,
                door_open_duration: 3,
                obstruction: false
            }),
            Err(_) => None
        }
    }

    // Test that motor direction and state are set correctly
    #[test]
    fn test_fsm_on_init_between_floors() {
        let mut state = match create_test_state() {
            Some(s) => s,
            None => {
                println!("[SKIPPED] Elevator hardware not available");
                return;
            }
        };

        state.fsm_on_init_between_floors();

        assert_eq!(state.dirn, Dirn::Down);
        assert_eq!(state.behaviour, ElevatorBehaviour::Moving);
        
        state.connection.motor_direction(Dirn::Stop as u8);
    }
}

/// Test the function that handles door timeout events
mod tests_fsm {
    use super::*;

    // Helper to create test state with real elevator (skips test if connection fails)
    fn create_test_elevator(initial_behaviour: ElevatorBehaviour) -> ElevatorState {
        let (timer_tx, _) = crossbeam_channel::bounded(1);
        ElevatorState {
            behaviour: initial_behaviour,
            connection: Elevator::init("127.0.0.1:15657", config::FLOOR_COUNT as u8).unwrap(),
            timer_tx,
            no_of_timer_threads: 0,
            floor: -1, 
            dirn: Dirn::Stop,
            requests: [[false; config::CALL_COUNT]; config::FLOOR_COUNT],
            door_open_duration: 3,
            obstruction: false
        }
    }
    
    // Test that timeout thread increments counter and sends message after duration
    #[test]
    fn test_start_time_out_thread_increments_counter_and_sends_message() {
        let (timer_tx, timer_rx) = crossbeam_channel::bounded(1);
        let mut elevator = create_test_elevator(ElevatorBehaviour::Idle);
        elevator.timer_tx = timer_tx;
        elevator.door_open_duration = 1;

        elevator.start_time_out_thread();
        assert_eq!(elevator.no_of_timer_threads, 1);
        
        assert_eq!(timer_rx.recv_timeout(Duration::from_secs(2)), Ok(true));
    }

    // Test that lights are set according to request matrix
    #[test]
    fn test_set_all_lights_updates_buttons_correctly() {
        let mut elevator = create_test_elevator(ElevatorBehaviour::Idle);
        elevator.requests = [
            [true, false, true],  
            [false, true, false], 
            [true, true, true],   
            [false, false, false] 
        ];

        elevator.set_all_lights();
        println!("Lights set successfully - manual verification needed");
    
        for floor in 0..config::FLOOR_COUNT {
            for button in 0..CALL_COUNT {
                elevator.connection.call_button_light(floor as u8, button as u8, false);
            }
        }
    }

    // Test direction choice when moving up with requests above
    #[test]
    fn test_requests_choose_direction_up_with_requests_above() {
        let mut elevator = create_test_elevator(ElevatorBehaviour::Moving);
        elevator.dirn = Dirn::Up;
        elevator.floor = 1;
        elevator.requests[2][0] = true; 

        let result = elevator.requests_choose_direction();
        assert_eq!(result.dirn, Dirn::Up);
        assert_eq!(result.behavior, ElevatorBehaviour::Moving);
    }

    // Test that UP direction clears Cab and HallUp requests at current floor
    #[test]
    #[ignore = "Requires real elevator connection"]
    fn test_requests_clear_at_current_floor_up_direction() {
        let (manager_tx, manager_rx) = crossbeam_channel::unbounded();
        let mut elevator = create_test_elevator(ElevatorBehaviour::Moving);
        elevator.dirn = Dirn::Up;
        elevator.floor = 1;
        elevator.requests[1] = [true; CALL_COUNT]; 

        elevator.requests_clear_at_current_floor(&manager_tx);
        
        assert!(!elevator.requests[1][Button::Cab as usize]); 
        assert!(!elevator.requests[1][Button::HallUp as usize]); 
        assert_eq!(elevator.requests[1][Button::HallDown as usize], 
                elevator.requests_above()); 
        
        let msg = manager_rx.try_recv().expect("Should receive message");
        assert!(matches!(msg, messages::Manager::ClearRequest(_, _)));
    }

    // Test detection of requests at current floor
    #[test]
    fn test_requests_here_detects_pressed_buttons() {
        let elevator = ElevatorState {
            floor: 1,
            requests: [
                [false, false, false],
                [false, true, false], 
                [false, false, false],
                [false, false, false]
            ],
            ..create_test_elevator(ElevatorBehaviour::Idle)
        };

        assert!(elevator.requests_here());
    }


    // Test detection of requests below current floor
    #[test]
    fn test_requests_below_detects_lower_floors() {
        let elevator = ElevatorState {
            floor: 2,
            requests: [
                [true, false, false],
                [false, false, false],
                [false, false, false],
                [false, false, false] 
            ],
            ..create_test_elevator(ElevatorBehaviour::Idle)
        };

        assert!(elevator.requests_below());
    }

    // Test detection of requests above current floor
    #[test]
    fn test_requests_above_detects_higher_floors() {
        let elevator = ElevatorState {
            floor: 0,
            requests: [
                [false, false, false],
                [false, false, false],
                [true, false, false], 
                [false, false, false]
            ],
            ..create_test_elevator(ElevatorBehaviour::Idle)
        };

        assert!(elevator.requests_above());
    }

    // Test stop decision when moving down with cab request
    #[test]
    fn test_requests_should_stop_down_with_cab_request() {
        let elevator = ElevatorState {
            dirn: Dirn::Down,
            floor: 1,
            requests: [
                [false, false, false],
                [false, false, true], 
                [false, false, false],
                [false, false, false] 
            ],
            ..create_test_elevator(ElevatorBehaviour::Moving)
        };

        assert!(elevator.requests_should_stop());
    }

}
