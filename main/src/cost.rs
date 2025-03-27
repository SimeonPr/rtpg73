//! Module for handling elevator request assignment using an external Hall Request Assigner (HRA) algorithm.
//!
//! This module provides functionality to:
//! - Convert elevator states and requests into a format suitable for the HRA
//! - Execute the external HRA executable
//! - Parse the HRA output into controller requests
//! - Identify elevators with assigned calls

/// Input structure for the Hall Request Assigner (HRA) algorithm.
///
/// Contains:
/// - `hall_requests`: 2D vector representing hall call buttons (up/down per floor)
/// - `states`: HashMap of elevator states with their current behavior, floor, and requests
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

/// Executes the external HRA executable with provided JSON input.
///
/// # Parameters
/// - `executable`: Path to the HRA executable
/// - `input_json`: JSON string containing the input data
///
/// # Returns
/// - `Some(Vec<u8>)`: Output from the executable if successful
/// - `None`: If execution fails or executable returns non-zero status
///
/// # Notes
/// - The executable is called with `--input` flag followed by the JSON string
/// - Both stdout and stderr are captured
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

/// Main elevator algorithm that coordinates request assignment.
///
/// # Parameters
/// - `world_view`: Current state of the elevator system including:
///   - Elevator states
///   - Hall requests
///   - Cab requests
///
/// # Returns
/// - `Some((ControllerRequests, Vec<i32>))`: Tuple containing:
///   - Assigned controller requests (2D array of bools per floor/direction)
///   - List of elevator IDs with assigned calls
/// - `None`: If any step fails (JSON conversion, executable execution, etc.)
///
/// # Workflow
/// 1. Collects elevator states and requests into HRAInput format
/// 2. Converts to JSON and passes to HRA executable
/// 3. Parses HRA output into controller requests
pub fn elevator_algorithm(world_view: &WorldView) -> Option<(fsm::ControllerRequests, Vec<i32>)>{
//    let mut states = HashMap::new();
    let alive_elevators = world_view.get_alive_elevators(2);
    let working_elevators: std::collections::HashSet<u8> = alive_elevators
        .iter()
        .filter(|id| {
            if let Some(elevator) = world_view.get_elevators().get(id) {
                elevator.is_working
        } else {
            false
        }
    })
    .cloned()
    .collect();
    if working_elevators.is_empty() {
        return Some(([[false; config::CALL_COUNT]; config::FLOOR_COUNT], vec![]));
    }

    let mut states = HashMap::new();


    let elevators = world_view.get_elevators();
    for id in working_elevators.iter() {
        let elevator = elevators.get(id)?;
        let key = format!("id_{}", id);
        let cab_requests_bool = elevator.get_cab_requests()
            .map(|s| matches!(s.get_state(), RequestState::Confirmed));
        
        let elevator_state = json!({
            "behaviour": elevator.state.behaviour,
            "floor": elevator.state.current_floor,
            "direction": elevator.state.dirn,
            "cabRequests": cab_requests_bool,
        });

       
        states.insert(key, elevator_state);
    }

   
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

    let all_reqs = covert_json_to_all_controller_reqs(&output_json)?;

    let mut active_ids: Vec<i32> = Vec::new();
    for (id, requests) in &all_reqs {
    let has_requests = requests.iter().any(|floor| floor.iter().any(|&call| call));
    if has_requests {
        active_ids.push(*id);
    }
}


let own_reqs = all_reqs
    .get(&(own_id as i32))
    .cloned()
    .unwrap_or([[false; config::CALL_COUNT]; config::FLOOR_COUNT]);
Some((own_reqs, active_ids))
}

/// Converts HRA output JSON into controller requests format.
///
/// # Parameters
/// - `output_json`: JSON string from HRA executable
/// - `id`: ID of the current elevator to extract requests for
///
/// # Returns
/// - `Ok((ControllerRequests, Vec<i32>))`: Parsed requests and active elevators
/// - `Err(serde_json::Error)`: If JSON parsing fails
///
/// # Errors
/// - If JSON structure is invalid
/// - If specified ID is not found in output
/// - If any value conversion fails
pub fn covert_json_to_all_controller_reqs(output_json: &str) -> Option<HashMap<i32, fsm::ControllerRequests>> {
    let parsed_json: Value = serde_json::from_str(output_json).ok()?;
    let mut assignments: HashMap<i32, fsm::ControllerRequests> = HashMap::new();

    for (key, val) in parsed_json.as_object()? {
        if let Some(id_str) = key.strip_prefix("id_") {
            let id: i32 = id_str.parse().ok()?;
            let mut controller_requests: fsm::ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];

            for (floor, bool_pair) in val.as_array()?.iter().enumerate() {
                let pair = bool_pair.as_array()?;
                controller_requests[floor][0] = pair[0].as_bool()?;
                controller_requests[floor][1] = pair[1].as_bool()?;
            }

            assignments.insert(id, controller_requests);
        }
    }

    Some(assignments)
}

/// Extracts elevator IDs that have assigned calls from HRA output.
///
/// # Parameters
/// - `parsed_json`: Parsed JSON Value from HRA output
///
/// # Returns
/// - `Some(Vec<i32>)`: List of elevator IDs with assigned calls
/// - `None`: If JSON structure is invalid
///
/// # Notes
/// - Only considers objects with keys starting with "id_"
/// - An elevator is included if any of its assigned calls is true
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

#[cfg(test)]

/// Test module for HRA executable execution
mod test_run_hra_executable {
    use super::*;

    #[test]
    fn test_run_hra_executable() {
        let json_data = r#"{
            "hallRequests": [[true, false], [false, true], [false, false], [false, false]],
            "states": {
                "id_1": {
                    "behaviour": "moving",
                    "floor": 2,
                    "direction": "up",
                    "cabRequests": [false, false, true, true]
                },
                "id_2": {
                    "behaviour": "idle",
                    "floor": 0,
                    "direction": "stop",
                    "cabRequests": [false, false, false, false]
                }
            }
        }"#;

        let output = run_hra_executable("./src/hall_request_assigner/hall_request_assigner", json_data);
        assert!(output.is_some());

        let output_str = String::from_utf8(output.unwrap()).unwrap();
        let output_json: serde_json::Value = serde_json::from_str(&output_str).unwrap();
        let expected_output = serde_json::json!({
            "id_1": [[false, false], [false, false], [false, false], [false, false]],
            "id_2": [[true, false], [false, true], [false, false], [false, false]]
          });
        assert_eq!(output_json, expected_output);
    }

    #[test]
    fn test_run_hra_executable_invalid_json() {
        let json_data = r#"{
            "hallRequests": [[true, false], [false, true], [false, false], [false, false]],
            "states": {
                "id_1": {
                    "behaviour": "moving",
                    "floor": 2,
                    "direction": "up",
                    "cabRequests": [false, false, true, true]
                },
                "id_2": {
                    "behaviour": "idle",
                    "floor": 0,
                    "direction": "stop",
                    "cabRequests": [false, false, false, false]
                }
            }
        "#;
    
        let output = run_hra_executable("./src/hall_request_assigner/hall_request_assigner", json_data);
        assert!(output.is_none());
    }

    #[test]
    fn test_run_hra_executable_invalid_executable() {
        let json_data = r#"{
            "hallRequests": [[true, false], [false, true], [false, false], [false, false]],
            "states": {
                "id_1": {
                    "behaviour": "moving",
                    "floor": 2,
                    "direction": "up",
                    "cabRequests": [false, false, true, true]
                },
                "id_2": {
                    "behaviour": "idle",
                    "floor": 0,
                    "direction": "stop",
                    "cabRequests": [false, false, false, false]
                }
            }
        }"#;
    
        let output = run_hra_executable("invalid_executable", json_data);
        assert!(output.is_none());
    }
}

/// Test module for elevator algorithm
mod test_elevator_algorithm {
    use super::*;

    #[test]
    fn test_elevator_algorithm_one_elevator() {
        let world_view = WorldView::init(1);
        
        // Set up elevator 1 state
        let (mut world_view, _) = world_view.handle_elevator_state(
            fsm::Dirn::Up,
            fsm::ElevatorBehaviour::Moving,
            2
        );
    
        // Set up elevator 2 state
        let mut elevators = world_view.get_elevators();
        let mut elevator1 = elevators.get(&1).unwrap().clone(); // Use ID = 1
        elevator1.state.behaviour = fsm::ElevatorBehaviour::Idle;
        elevator1.state.current_floor = 0;
        elevator1.state.dirn = fsm::Dirn::Stop;
        elevators.insert(1, elevator1); // Use ID = 1
        world_view = WorldView {
            id: world_view.get_id(),
            elevators,
            hall_requests: world_view.get_hall_requests()
        };
    
        // Set hall requests
        let mut hall_requests = world_view.get_hall_requests();
        hall_requests[0][0].set_to(RequestState::Confirmed, world_view.get_id());
        hall_requests[1][1].set_to(RequestState::Confirmed, world_view.get_id());
        world_view = WorldView {
            id: world_view.get_id(),
            elevators: world_view.get_elevators(),
            hall_requests
        };
    
        // Debug output
        println!("WorldView before algorithm:");
        println!("Elevators: {:?}", world_view.get_elevators());
        println!("Hall Requests: {:?}", world_view.get_hall_requests());
    
        let result = elevator_algorithm(&world_view);
        println!("Algorithm result: {:?}", result);
    
        assert!(result.is_some(), "elevator_algorithm returned None, expected Some");
    
        if let Some((controller_reqs, assigned_elevators)) = result {
            assert_eq!(controller_reqs, [
                [true, false, false],
                [false, true, false],
                [false, false, false],
                [false, false, false]
            ]);
            assert_eq!(assigned_elevators, vec![1]);
        }
    }

    #[test]
    fn test_elevator_algorithm_no_alive_elevators() {
        let world_view = WorldView::init(1);
        let result = elevator_algorithm(&world_view);
        assert!(result.is_none());
    }

    #[test]
    fn test_elevator_algorithm_no_hall_requests() {
        let world_view = WorldView::init(1);
        
        // Set up elevator states but no hall requests
        let (world_view, _) = world_view.handle_elevator_state(
            fsm::Dirn::Stop,
            fsm::ElevatorBehaviour::Idle,
            0
        );

        let result = elevator_algorithm(&world_view);
        assert!(result.is_some());
        
        let (controller_reqs, assigned_elevators) = result.unwrap();
        assert_eq!(controller_reqs, [
            [false, false, false],
            [false, false, false],
            [false, false, false],
            [false, false, false]
        ]);
        assert!(assigned_elevators.is_empty());
    }
}

/// Test module for JSON to controller requests conversion
mod test_conver_json_to_controller_reqs {
    use super::*;

    #[test]
    fn test_covert_json_to_controller_reqs() {
        let json_data = r#"{
            "id_1": [[false, false], [false, false], [false, false], [false, false]],
            "id_2": [[true, false], [false, true], [false, false], [false, false]]
        }"#;

        let result = covert_json_to_controller_reqs(json_data, 2).unwrap();
        let expected_output = (
            [
                [true, false, false],
                [false, true, false],
                [false, false, false],
                [false, false, false]
            ],
            vec![2]
        );
        assert_eq!(result, expected_output);
    }

    #[test]
    fn test_covert_json_to_controller_reqs_invalid_json() {
        let json_data = r#"{
            "id_1": [[false, false], [false, false], [false, false], [false, false],
            "id_2": [[true, false], [false, true], [false, false], [false, false]]
        }"#;

        let result = covert_json_to_controller_reqs(json_data, 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_covert_json_to_controller_reqs_invalid_id() {
        let json_data = r#"{
            "id_1": [[false, false], [false, false], [false, false], [false, false]],
            "id_2": [[true, false], [false, true], [false, false], [false, false]]
        }"#;

        let result = covert_json_to_controller_reqs(json_data, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_covert_json_to_controller_reqs_invalid_pair() {
        let json_data = r#"{
            "id_1": [[false, false], [false, false], [false, false], [false, false]],
            "id_2": [[true, false], [false, true], [false, false], [false, false]]
        }"#;

        let result = covert_json_to_controller_reqs(json_data, 2).unwrap();
        let expected_output = (
            [
                [true, false, false],
                [false, true, false],
                [false, false, false],
                [false, false, false]
            ],
            vec![2]
        );
        assert_eq!(result, expected_output);
    }
}

/// Test module for ID extraction from HRA output
mod test_ids_with_assigned_calls {
    use super::*;

    #[test]
    fn test_ids_with_assigned_calls() {
        let json = r#"{
            "id_1": [[true, false], [false, false], [false, false], [false, false]],
            "id_2": [[false, false], [false, false], [false, false], [false, false]],
            "id_3": [[false, false], [false, false], [false, false], [false, false]]
        }"#;

        let parsed_json: Value = serde_json::from_str(json).unwrap();
        let result = ids_with_assigned_calls(&parsed_json).unwrap();
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_ids_with_assigned_calls_multiple_ids() {
        let json = r#"{
            "id_1": [[true, false], [false, false], [false, false], [false, false]],
            "id_2": [[false, false], [false, false], [false, false], [false, false]],
            "id_3": [[true, false], [false, false], [false, false], [false, false]]
        }"#;

        let parsed_json: Value = serde_json::from_str(json).unwrap();
        let result = ids_with_assigned_calls(&parsed_json).unwrap();
        assert_eq!(result, vec![1, 3]);
    }

    #[test]
    fn test_ids_with_assigned_calls_no_true() {
        let json = r#"{
            "id_1": [[false, false], [false, false], [false, false], [false, false]],
            "id_2": [[false, false], [false, false], [false, false], [false, false]],
            "id_3": [[false, false], [false, false], [false, false], [false, false]]
        }"#;

        let parsed_json: Value = serde_json::from_str(json).unwrap();
        let result = ids_with_assigned_calls(&parsed_json).unwrap();
        assert_eq!(result, Vec::<i32>::new());
    }

    #[test]
    #[should_panic]
    fn test_ids_with_assigned_calls_invalid_json() {
        let json = r#"{
            "id1": [[true, false], [false, false], [false, false], [false, false]],
            "id_2": [[false, false], [false, false], [false, false], [false, false]],
            "id_3": [[false, false], [false, false], [false, false] [false, false]]
        }"#;

        let parsed_json: Value = serde_json::from_str(json).unwrap();
        let result = ids_with_assigned_calls(&parsed_json).unwrap();
        assert_eq!(result, vec![1, 3]);
    }

    #[test]
    fn ids_with_assigned_calls_wrong_id() {
        let json = r#"{
            "id1": [[true, false], [false, false], [false, false], [false, false]],
            "id_2": [[false, false], [false, false], [false, false], [false, false]],
            "id_3": [[false, false], [false, false], [false, false], [false, false]]
        }"#;

        let parsed_json: Value = serde_json::from_str(json).unwrap();
        let result = ids_with_assigned_calls(&parsed_json).unwrap();
        assert!(result.is_empty()); 
    }
}
