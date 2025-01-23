use driver_rust::elevio::elev::Elevator;

#[derive(Debug)]
pub enum ElevatorBehaviour {
    Idle,
    DoorOpen,
    Moving
}
#[derive(Debug)]
pub enum Dirn {
    Down = -1,
    Stop = 0,
    Up = 1
}
#[derive(Debug)]
pub enum Button {
    HallUp,
    HallDown,
    Cab
}
#[derive(Debug)]
pub struct ElevatorState {
    floor: u8,
    dirn: Dirn,
    requests: [[i32; 4]; 3],
    behaviour: ElevatorBehaviour,
    door_open_duration: f64,
    connection: Elevator
}
impl ElevatorState {
    pub fn fsm_on_init_between_floors(&mut self) {
        //motor direction
        self.connection.motor_direction(Dirn::Down as u8);
        self.dirn = Dirn::Down;
        self.behaviour = ElevatorBehaviour::Moving;
    }
    pub fn fsm_on_request_button_press(&mut self, floor: u8, call: u8) {
        match self.behaviour {
            ElevatorBehaviour::Idle => {
            },
            ElevatorBehaviour::DoorOpen => {},
            ElevatorBehaviour::Moving => {}
        };
    }

    pub fn fsm_on_floor_arrival(&mut self, floor: u8) {
        self.floor = floor;
        self.connection.floor_indicator(self.floor);

        match self.behaviour {
            ElevatorBehaviour::Moving => {
                if self.requests_should_stop() {
                    self.connection.motor_direction(Dirn::Stop as u8);
                    self.connection.door_light(true);
                    self.requests_clear_at_current_floor();
                    // timer
                    //self.set_all_light();
                    self.behaviour = ElevatorBehaviour::DoorOpen;
                }
            }
            _ => {},
        };
        self.connection.motor_direction(Dirn::Stop as u8);
    }
    fn requests_clear_at_current_floor(&mut self) {
        for b in 0..3 {
            self.requests[(self.floor - 1) as usize][b as usize] = 0;
        }
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
    pub fn fsm_on_stop_button_press(&mut self){}

    pub fn init_elevator(elevator_connection: Elevator) -> ElevatorState {
        ElevatorState {
            floor: u8::MAX,
            dirn: Dirn::Stop,
            requests: [[0;4]; 3],
            behaviour: ElevatorBehaviour::Idle,
            door_open_duration: 3.0,
            connection: elevator_connection
        }
    }
}
