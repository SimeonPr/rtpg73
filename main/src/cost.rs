use log::debug;
use serde::{Serialize, Deserialize};
use serde_json;
use std::collections::HashMap;
use serde_json::{Value, from_str, json};
use std::process::{Command,Stdio};
use std::io::Write;
use crate::fsm;
use crate::config;

use crate::manager::{RequestState,ElevatorNetworkState,WorldView, Elevator};

//new end
#[derive(serde::Serialize)]
struct HRAInput {
    #[serde(rename = "hallRequests")]
    pub hall_requests: Vec<Vec<bool>>,
    pub states: HashMap<String, Value>
}

fn run_hra_executable(executable: &str, input_json: &str) -> Vec<u8>{
    let child = Command::new(executable)
        .arg("--input")
        .arg(input_json)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start the elevator algorythm");

    let output = child 
        .wait_with_output()
        .expect("Failed to read output");
    if !output.status.success(){
        eprintln!("error running the HRA executable {:?}", output);
        panic!("Command failed");
    }
    output.stdout
}


pub fn elevator_algorythm(world_view: &WorldView) -> fsm::ControllerRequests {
    // Create a HashMap to s#[derive(serde::Serialize)]tore the elevator states
    let mut states = HashMap::new();
    // Extract elevator state and hall requests from the matrix
    let alive_elevators = world_view.get_alive_elevators(2);
    let elevators = world_view.get_elevators();
    for id in alive_elevators.iter() {
        let elevator = elevators.get(id).unwrap();
        let key = format!("id_{}", id);
        let cab_requests_bool = elevator.get_cab_requests().map(|s| matches!(s.get_state(), RequestState::Confirmed));
        // Use serde_json::json! to construct the elevator state object
        let elevator_state = json!({
            "behaviour": elevator.state.behaviour,
            "floor": elevator.state.current_floor,
            "direction": elevator.state.dirn,
            "cabRequests": cab_requests_bool,
        });

        // Insert the elevator state into the HashMap with the generated key
        states.insert(key, elevator_state);
    }

     // Create hall requests dynamically
    let hall_requests = world_view.get_hall_requests().iter().map(|row| row.iter().map(|s| matches!(s.get_state(), RequestState::Confirmed)).collect()).collect();
     let input = HRAInput {
        hall_requests,
        states,
     };


    // Serialize to JSON string
    let json_string = serde_json::to_string(&input).unwrap();

    let hra_executable = "./src/hall_request_assigner/hall_request_assigner";

    // Call the external executable with the JSON input
    debug!("{}", json_string);
    let output = run_hra_executable(&hra_executable, &json_string);
    let id1 = world_view.get_id();
    
     let a = String::from_utf8(output).unwrap();
    let mut controller_requests: fsm::ControllerRequests = covert_json_to_controller_reqs(&a, id1);
     let tmp = world_view.get_confirmed_requests();
     for floor in 0..config::FLOOR_COUNT {
        controller_requests[floor][2] = tmp[floor][2];  
     }
    controller_requests

}

pub fn covert_json_to_controller_reqs(output_json: &str, id: u8) -> fsm::ControllerRequests {
    // Parse the JSON string into a Value
    let parsed_json: Value = from_str(output_json).unwrap();

    // Initialize a matrix with 4 rows (for id_1 to id_4), each being a vector of boolean pairs
    let mut controller_requests: fsm::ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];

    // Iterate over possible IDs from 1 to 4

    for i in 1..5 {
        // Check if the ID exists in the parsed JSON
        if let Some(id_data) = parsed_json.get(&format!("id_{}", i)) {
            if i != id {
                continue;
            }
            // Iterate over the boolean pairs in the array for the current ID
            for (floor, bool_pair) in id_data.as_array().unwrap().iter().enumerate() {
                // Each pair is an array of two booleans
                if let Some(pair) = bool_pair.as_array() {
                    let bool1 = pair[0].as_bool().unwrap();
                    let bool2 = pair[1].as_bool().unwrap();
                    // Add the pair to the appropriate row in the matrix
                    controller_requests[floor][0] = controller_requests[floor][0] || bool1;
                    controller_requests[floor][1] = controller_requests[floor][1] || bool2;
                }
            }
        }
    }
    
    controller_requests
}


