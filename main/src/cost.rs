use serde_json;
use std::collections::HashMap;
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use crate::config;

use crate::models::{RequestState, WorldView, ControllerRequests};

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

pub fn elevator_algorithm(world_view: &WorldView) -> Option<(ControllerRequests, Vec<i32>)> {
    let alive_elevators = world_view.get_alive_elevators(2);
    if alive_elevators.is_empty() {
        return Some(([[false; config::CALL_COUNT]; config::FLOOR_COUNT], vec![]));
    }

    let mut states = HashMap::new();
    let elevators = world_view.elevators();
    for id in alive_elevators.iter() {
        let elevator = elevators.get(id)?;
        let key = format!("id_{}", id);
        let cab_requests_bool: Vec<bool> = elevator.cab_requests()
            .iter()
            .map(|s| matches!(s.state(), RequestState::Confirmed))
            .collect();
        let state = elevator.state;
        let elevator_state = json!({
            "behaviour": state.behaviour,
            "floor": state.current_floor,
            "direction": state.dirn,
            "cabRequests": cab_requests_bool,
        });

        states.insert(key, elevator_state);
    }

    let hall_requests: Vec<Vec<bool>> = world_view.hall_requests()
        .iter()
        .map(|row| row.iter()
            .map(|s| matches!(s.state(), RequestState::Confirmed))
            .collect())
        .collect();
    let input = HRAInput {
        hall_requests,
        states,
    };

    let json_string = serde_json::to_string(&input).ok()?;
    let hra_executable = "./src/hall_request_assigner/hall_request_assigner";
    let output = run_hra_executable(&hra_executable, &json_string)?;
    let own_id = world_view.id();
    let output_json = String::from_utf8(output).ok()?;
    let all_reqs = covert_json_to_all_controller_reqs(&output_json)?;

    let mut active_ids: Vec<i32> = Vec::new();
    for (id, requests) in &all_reqs {
        let has_requests = requests.iter().any(|floor| floor.iter().any(|&call| call));
        if has_requests {
            active_ids.push(*id);
        }
    }

    let own_reqs = all_reqs.get(&(own_id as i32))?.clone();
    Some((own_reqs, active_ids))
}

pub fn covert_json_to_all_controller_reqs(output_json: &str) -> Option<HashMap<i32, ControllerRequests>> {
    let parsed_json: Value = serde_json::from_str(output_json).ok()?;
    let mut assignments: HashMap<i32, ControllerRequests> = HashMap::new();

    for (key, val) in parsed_json.as_object()? {
        if let Some(id_str) = key.strip_prefix("id_") {
            let id: i32 = id_str.parse().ok()?;
            let mut controller_requests: ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];

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
