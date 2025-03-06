use serde::{Serialize, Deserialize};
use serde_json;
use std::collections::HashMap;
use serde_json::{Value, from_str, json};
use std::process::{Command,Stdio};
use std::io::Write;
use crate::fsm::ControllerRequests;
use crate::config;

use crate::manager::{RequestState,ElevatorNetworkState,ManagerRequests,WorldView, Elevator}

//new end



fn run_hra_executable(executable: &str, input_sjon_ &[u8]) -> Vec<u8>{
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start the elevator algorythm");

    if let Some(mut stdin) = child.stdin.take(){
        stdin.write_all(input_json).expect("Failed to write to stdin");
    }

    let output = child 
        .wait_with-output()
        .expect("Failed to read output");
    if !output.status.success(){
        eprintln!("error running the HRA executable {:?}", output);
        panic!("Command failed");
    }
    output.stdout
}


pub fn elevator_algorythm(world_view: &WorldView, hallcalls: &Hall_calls) -> ControllerRequests {
    // Create a HashMap to store the elevator states
    let mut states = HashMap::new();
    let mut hall_request = Hashmap::new();
    let current_time = SystemTime::now();
    // Extract elevator state and hall requests from the matrix
    
    for(id, elevator) in &world_view.elevators{
        if let Ok(elapsed) = elevator.last_received.elapsed(){
            if elapsed > Duration::from_secs(1) && *id !=world_view.id{
                continue;
            }
        }
    
    
        
        let key = format!("id_{}", id);

        // Use serde_json::json! to construct the elevator state object
        let elevator_state = json!({
            "behavior": elevator.state.behavior,
            "floor": elevator.state.current_floor,
            "direction": elevator.state.dirn,
            "cabRequests": elevator.cab_requests, ///this cab_requests currently dont exist.
        });

        // Insert the elevator state into the HashMap with the generated key
        states.insert(key, elevator_state);
    }

     // Create hall requests dynamically
 
     let input = HRAInput {
        hallcalls,
        states,
     };


    // Serialize to JSON string
    let json_string = serde_json::to_string(&input).unwrap();

    let json_bytes = json_string.as_bytes();

    let hra_executable = "./hall_request_assigner/hall_request_assigner";

    // Call the external executable with the JSON input
    let output = run_hra_executable(&hra_executable, json_bytes);
    let id1 = world_view.id.clone();

    let mut controller_requests: ControllerRequests = covert_json_to_controller_reqs(output, id1);

    controller_requests;

}

pub fn covert_json_to_controller_reqs(output_json: &str, id: i8) -> ControllerRequests {
    // Parse the JSON string into a Value
    let parsed_json: Value = from_str(output_json).unwrap();

    // Initialize a matrix with 4 rows (for id_1 to id_4), each being a vector of boolean pairs
    let mut controller_requests: ControllerRequests = [[false; config::CALL_COUNT]; config::FLOOR_COUNT];

    // Iterate over possible IDs from 1 to 4

    for i in 1..5 {
        // Check if the ID exists in the parsed JSON
        if let Some(id_data) = parsed_json.get(&format!("id_{}", i)) {
            if(i != id){
                continue;
            }
            // Iterate over the boolean pairs in the array for the current ID
            for (floor, bool_pair) in id_data.as_array().unwrap().iter.enumerate() {
                // Each pair is an array of two booleans
                if let Some(pair) = bool_pair.as_array() {
                    let bool1 = pair[0].as_bool().unwrap();
                    let bool2 = pair[1].as_bool().unwrap();
                    // Add the pair to the appropriate row in the matrix
                    controller_request[floor][0] = controller_request[floor][0] || bool1;
                    controller_request[floor][1] = controller_request[floor][1] || bool2;
                }
            }
        }
    }
    
    controller_requests
}

/*Example for Main: 

let status_matrix = vec![
        vec![true, true, false, true, false],  // id1 behavior and cab requests
        vec![false, false, true, true, false],  // id2 behavior and cab requests
        vec![true, false, true, false, true],  // id3 behavior and cab requests
        vec![false, true, false, true, false],  // id4 behavior and cab requests
        vec![true, false, true, false],  // hall requests for floor 1
        vec![false, true, false, true],  // hall requests for floor 2
        vec![true, true, false, false],  // hall requests for floor 3
        vec![false, false, true, true],  // hall requests for floor 4
    ];

    // Convert the matrix to JSON
    let input_json = convert_matrix_to_json(status_matrix);

    // Call the cost function with the JSON input
    let output_json = cost_func(&input_json);

    // Convert the output JSON back into a matrix
    let output_matrix = convert_json_to_matrix(&output_json);

*/ 
