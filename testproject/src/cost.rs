use serde::{Serialize, Deserialize};
use serde_json;
use std::collections::HashMap;
use serde_json::{Value, from_str, json};


#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevatorState {
    pub id: i8,
    pub behavior: String,  // e.g., "idle", "moving", "doorOpen"
    pub floor: usize,
    pub direction: String,  // e.g., "up", "down", "stop"
    pub cab_requests: Vec<bool>,  // [cabreq1, cabreq2, cabreq3, cabreq4]
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Elevators {
    pub elevators: HashMap<i8, ElevatorState>, // Map of elevators by ID
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HallCalls {
    pub up: Vec<bool>,   // Floors with UP hall calls
    pub down: Vec<bool>, // Floors with DOWN hall calls
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HRAInput {
    pub hall_requests: Vec<Vec<bool>>,  // Hall request matrix [floor1up, floor1down, ...]
    pub states: HashMap<String, Value>,  // Map of elevator states keyed by id
}

#[derive(Debug, Serialize, Deserialize)]
struct OutputMatrix {
    // Define the output matrix structure you expect from the cost function
    pub data: Vec<Vec<String>>,  // A matrix of strings as output
}

impl Elevators {
    // Accessing or modifying an elevator by ID
    pub fn change_floor(&mut self, id: i8, new_floor: usize) {
        if let Some(elevator) = self.elevators.get_mut(&id) {
            elevator.floor = new_floor;
        }
    }
}






pub fn convert_matrix_to_json(elevators: &Elevators, hallcalls: &HallCalls) -> String {
    // Create a HashMap to store the elevator states
    let mut states = HashMap::new();
    
    // Extract elevator state and hall requests from the matrix
    for(id,elevstate) in &elevators.elevators {
        if *id == 0{
            continue;
        }

    

        let key = format!("id_{}", id);

        // Use serde_json::json! to construct the elevator state object
        let elevator_state = json!({
            "behavior": elevstate.behavior,
            "floor": elevstate.floor,
            "direction": elevstate.direction,
            "cabRequests": elevstate.cab_requests,
        });

        // Insert the elevator state into the HashMap with the generated key
        states.insert(key, elevator_state);
    }

     // Create hall requests dynamically
     let mut hall_requests: Vec<Vec<bool>> = Vec::new();

    // Iterate through each floor and create the hall request matrix
    for i in 0..hallcalls.up.len() {
        let mut floor_requests = Vec::new();

        // Check if there is an up hall call for the current floor
        floor_requests.push(*hallcalls.up.get(i).unwrap_or(&false));

        // Check if there is a down hall call for the current floor
        floor_requests.push(*hallcalls.down.get(i).unwrap_or(&false));

        // Add the floor requests to the hall request matrix
        hall_requests.push(floor_requests);
    }
 
     let input = HRAInput {
         hall_requests,
         states,
     };


    // Serialize to JSON string
    let json_string = serde_json::to_string(&input).unwrap();

    // Print the generated JSON for debugging
    println!("Generated JSON: {}", json_string);
    json_string
}

pub fn convert_json_to_matrix(output_json: &str) -> Vec<Vec<bool>> {
    // Parse the JSON string into a Value
    let parsed_json: Value = from_str(output_json).unwrap();

    // Initialize a matrix with 4 rows (for id_1 to id_4), each being a vector of boolean pairs
    let mut matrix: Vec<Vec<bool>> = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    // Iterate over possible IDs from 1 to 4
    for i in 1..5 {
        // Check if the ID exists in the parsed JSON
        if let Some(id_data) = parsed_json.get(&format!("id_{}", i)) {
            // Iterate over the boolean pairs in the array for the current ID
            for bool_pair in id_data.as_array().unwrap() {
                // Each pair is an array of two booleans
                if let Some(pair) = bool_pair.as_array() {
                    let bool1 = pair[0].as_bool().unwrap();
                    let bool2 = pair[1].as_bool().unwrap();
                    // Add the pair to the appropriate row in the matrix
                    matrix[i - 1].push(bool1);
                    matrix[i - 1].push(bool2);
                }
            }
        }
    }
    
    matrix
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
