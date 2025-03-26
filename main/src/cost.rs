use serde_json;
use std::collections::HashMap;
use std::collections::HashSet;
use serde_json::{Value, from_str, json};
use std::process::{Command,Stdio};
use crate::fsm;
use crate::config;

use crate::manager::{RequestState, WorldView};

#[derive(serde::Serialize)]
struct HRAInput {
    #[serde(rename = "hallRequests")]
    pub hall_requests: Vec<Vec<bool>>,
    pub states: HashMap<String, Value>
}

fn run_hra_executable(executable: &str, input_json: &str) -> Option<Vec<u8>> {
    let child = Command::new(executable)
        .arg("--input")
        .arg(input_json)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let output = child 
        .wait_with_output()
        .ok()?;
    
    if !output.status.success() {
        eprintln!("error running the HRA executable {:?}", output);
        return None;
    }
    
    Some(output.stdout)
}


pub fn elevator_algorithm(world_view: &WorldView, ids: &HashSet<u8>) -> Option<HashMap<u8,fsm::ControllerRequests>> {
    let mut states = HashMap::new();
    let elevators = world_view.get_elevators();
    for id in ids.iter() {
        let elevator = elevators.get(id)?;
        let key = format!("{}", id);
        let cab_requests_bool = elevator.get_cab_requests()
            .map(|s| matches!(s.get_state(), RequestState::Confirmed));
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
    let hall_requests = world_view.get_hall_requests()
        .iter()
        .map(|row| row.iter()
            .map(|s| matches!(s.get_state(), RequestState::Confirmed))
            .collect())
        .collect();
    let input = HRAInput {
        hall_requests,
        states,
    };
    
    let json_string = serde_json::to_string(&input).ok()?;
    let hra_executable = "./src/hall_request_assigner/hall_request_assigner";
    
    let output = run_hra_executable(&hra_executable, &json_string)?;
    
    let output_json = String::from_utf8(output).ok()?;
    let mut result: HashMap<u8, fsm::ControllerRequests> = HashMap::new();
    for &id in ids {
        let mut to_insert = covert_json_to_controller_reqs(&output_json, id)?;
        let elevator = elevators.get(&id)?;
        for (floor, request) in elevator.get_cab_requests().iter().enumerate() {
            if request.get_state() == RequestState::Confirmed {
                to_insert[floor][2] = true;
            }
        }
        result.insert(id, to_insert);
    }
    Some(result)
}

pub fn covert_json_to_controller_reqs(output_json: &str, id: u8) -> Option<fsm::ControllerRequests> {
    // Parse the JSON string into a Value
    let parsed_json: Value = from_str(output_json).ok()?;

    // Initialize a matrix with 4 rows (for id_1 to id_4), each being a vector of boolean pairs
    let mut controller_requests: fsm::ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];

    // Check if the ID exists in the parsed JSON
    let id_data = parsed_json.get(&format!("{}", id))?;

    // Iterate over the boolean pairs in the array for the current ID
    for (floor, bool_pair) in id_data.as_array()?.iter().enumerate() {
        // Each pair is an array of two booleans
        let pair = bool_pair.as_array()?;
        let bool1 = pair[0].as_bool()?;
        let bool2 = pair[1].as_bool()?;

        // Add the pair to the appropriate row in the matrix
        controller_requests[floor][0] = bool1;
        controller_requests[floor][1] = bool2;
    }

    Some(controller_requests)
}
