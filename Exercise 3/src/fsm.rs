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
pub struct ElevatorState {
    floor: i8,
    dirn: Dirn,
    requests: [[i32; 4]; 4],
    behaviour: ElevatorBehaviour,
    door_open_duration: f64
}
impl ElevatorState {
    pub fn fsm_on_request_button_press(&mut self, floor: u8, call: u8) {
        match self.behaviour {
            ElevatorBehaviour::Idle => {
                self.requests[floor][call] = 1;
                set_all_lights(); 
            },
            ElevatorBehaviour::DoorOpen => {},
            ElevatorBehaviour::Moving => {}
        };
    }

    pub fn fsm_on_floor_arrival(&mut self, floor: u8) {}

    pub fn fsm_on_stop_button_press(&mut self){}

    pub fn init_elevator() -> ElevatorState {
        ElevatorState {
            floor: -1,
            dirn: Dirn::Stop,
            requests: [[0;4]; 4],
            behaviour: ElevatorBehaviour::Idle,
            door_open_duration: 3.0
        }
    }
}
