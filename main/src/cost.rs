use serde_json;
use std::collections::HashMap;
use serde_json::{Value, json};
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


pub fn elevator_algorithm(world_view: &WorldView) -> Option<(fsm::ControllerRequests, Vec<i32>)>{
    
    let alive_elevators = world_view.get_alive_elevators(2);
    if alive_elevators.is_empty() {
        return Some(([[false; config::CALL_COUNT]; config::FLOOR_COUNT], vec![]));
    }
    let mut states = HashMap::new();
    let elevators = world_view.get_elevators();
    for id in alive_elevators.iter() {
        let elevator = elevators.get(id)?;
        let key = format!("id_{}", id);
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
    
    let own_id = world_view.get_id();
    let output_json = String::from_utf8(output).ok()?;

    covert_json_to_controller_reqs(&output_json, own_id).ok()
}

pub fn covert_json_to_controller_reqs(
    output_json: &str,
    id: u8,
) -> Result<(fsm::ControllerRequests, Vec<i32>), serde_json::Error> {
    let parsed_json: Value = serde_json::from_str(output_json)?;

    let active_elevators = ids_with_assigned_calls(&parsed_json)
        .ok_or_else(|| serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "Failed to get active elevators")))?;

    let mut controller_requests: fsm::ControllerRequests =
        [[false; config::CALL_COUNT]; config::FLOOR_COUNT];

    let id_data = parsed_json
        .get(&format!("id_{}", id))
        .ok_or_else(|| serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "id not found")))?;

    for (floor, bool_pair) in id_data
        .as_array()
        .ok_or_else(|| serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "id_data is not an array")))?
        .iter()
        .enumerate()
    {
        // <-- Add this part back clearly!
        let pair = bool_pair
            .as_array()
            .ok_or_else(|| serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "pair is not an array")))?;

        controller_requests[floor][0] = pair.get(0).and_then(|b| b.as_bool()).unwrap_or(false);
        controller_requests[floor][1] = pair.get(1).and_then(|b| b.as_bool()).unwrap_or(false);
    }

    // Move this outside the loop clearly:
    Ok((controller_requests, active_elevators))
}

fn ids_with_assigned_calls(parsed_json: &Value) -> Option<Vec<i32>> {
    Some(
        parsed_json.as_object()?
            .iter()
            .filter_map(|(key, val)| {
                if key.starts_with("id_") {
                    let has_true = val.as_array()
                        .map(|arr| {
                            arr.iter().any(|inner_arr| {
                                inner_arr.as_array()
                                    .map(|bool_vals| {
                                        bool_vals.iter().any(|bool_val| bool_val.as_bool().unwrap_or(false))
                                    })
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);

                    if has_true {
                        key.trim_start_matches("id_").parse::<i32>().ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect(),
    )
}