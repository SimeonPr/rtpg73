use driver_rust::elevio::elev::Elevator;

use crate::timer::{self, Timer};

#[derive(Debug)]
enum ElevatorBehaviour {
    Idle,
    DoorOpen,
    Moving
}
#[derive(Debug, Copy, Clone)]
enum Dirn {
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
    floor: i8,
    dirn: Dirn,
    requests: [[i32; 3]; 4],
    behaviour: ElevatorBehaviour,
    door_open_duration: f64,
    connection: Elevator
}
struct DirectionBehaviourPair {
    dirn: Dirn,
    behavior: ElevatorBehaviour
}
impl ElevatorState {
    
    pub fn init_elevator(elevator_connection: Elevator) -> ElevatorState {
        ElevatorState {
            floor: -1,
            dirn: Dirn::Stop,
            requests: [[0;3]; 4],
            behaviour: ElevatorBehaviour::Idle,
            door_open_duration: 3.0,
            connection: elevator_connection
        }
    }
    
    pub fn fsm_on_init_between_floors(&mut self) {
        //motor direction
        self.connection.motor_direction(Dirn::Down as u8);
        self.dirn = Dirn::Down;
        self.behaviour = ElevatorBehaviour::Moving;
    }
    
    pub fn fsm_on_request_button_press(&mut self, floor: i8, call: u8, timer: &mut timer::Timer) {
        match self.behaviour {
            ElevatorBehaviour::DoorOpen => {
                if self.requests_should_clear_immediately(floor, call) {
                    timer.start(self.door_open_duration);
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
                        timer.start(self.door_open_duration);
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

    pub fn fsm_on_door_time_out(&mut self, timer: &mut Timer) {
        match self.behaviour {
            ElevatorBehaviour::DoorOpen => {
                let pair: DirectionBehaviourPair = self.requests_choose_direction();
                self.dirn = pair.dirn;
                self.behaviour = pair.behavior;

                match self.behaviour {
                    ElevatorBehaviour::DoorOpen => {
                        timer.start(self.door_open_duration);
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
    
    pub fn fsm_on_floor_arrival(&mut self, floor: i8, timer: &mut timer::Timer) {
        self.floor = floor;
        self.connection.floor_indicator(self.floor as u8);

        match self.behaviour {
            ElevatorBehaviour::Moving => {
                if self.requests_should_stop() {
                    self.connection.motor_direction(Dirn::Stop as u8);
                    self.connection.door_light(true);
                    self.requests_clear_at_current_floor();
                    // timer
                    timer.start(self.door_open_duration);
                    //self.set_all_light();
                    self.behaviour = ElevatorBehaviour::DoorOpen;
                }
            }
            _ => {},
        };
    }

    pub fn fsm_on_stop_button_press(&mut self){}

    fn requests_should_clear_immediately(&mut self, floor: i8, _call: u8) -> bool {
         self.floor == floor
    }
    
    fn set_all_lights(&self) {
        for f in 0..4 {
            for b in 0..3 {
                self.connection.call_button_light(f, b, self.requests[f as usize][b as usize] == 1);
            }
        }
    }
    
    fn requests_choose_direction(&mut self) -> DirectionBehaviourPair {
        match self.dirn {
            Dirn::Up => {
                if self.requests_above() {
                    DirectionBehaviourPair {dirn: Dirn::Up, behavior: ElevatorBehaviour::Moving}
                } else if self.requests_here() {
                    DirectionBehaviourPair {dirn: Dirn::Down, behavior: ElevatorBehaviour::DoorOpen}
                } else if self.requests_above() {
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
        for b in 0..3 {
            self.requests[self.floor as usize][b as usize] = 0;
        }
    }
    
    fn requests_here(&self) -> bool {
        for b in 0..3 {
            if self.requests[self.floor as usize][b as usize] == 1 {
                return true;
            }
        }
        return false;
    }
    
    fn requests_below(&self) -> bool {
        for f in 0..self.floor {
            for b in 0..3 {
                if self.requests[f as usize][b as usize] == 1 {
                    return true;
                }
            }
        }
        return false;
    }
    
    fn requests_above(&self) -> bool {
        for f in self.floor..4 {
            for b in 0..3 {
                if self.requests[f as usize][b as usize] == 1 {
                    return true;
                }
            }
        }
        return false;
    }
    
    fn requests_should_stop(&self) -> bool {
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
