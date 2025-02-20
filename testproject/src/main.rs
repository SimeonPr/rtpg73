use std::process::{Command, Output};
use std::collections::HashMap;
mod cost;




fn run_hra_executable(executable: &str, input_json: &[u8]) -> Vec<u8> {
    let output: Output = Command::new(executable)
        .arg("-i")
        .arg(String::from_utf8_lossy(input_json).to_string())
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        eprintln!("Error running the HRA executable: {:?}", output);
        panic!("Command failed");
    }

    output.stdout
}

fn main() {
    let hra_executable = ".\\hall_request_assigner\\hall_request_assigner.exe";

    let mut elevators = cost::Elevators{
        elevators: HashMap::new(),
    };
       
    elevators.elevators.insert(1, cost::ElevatorState {
        id: 1,
        behavior: String::from("idle"),
        floor: 0,
        direction: String::from("up"),
        cab_requests: vec![false, true, false, false],
    });
    
    
    let hall_calls = cost::HallCalls {
        up: vec![false, true, false, false],    
        down: vec![false, false, false, false],  
    };
    
    elevators.change_floor(1,2);
    
    // Convert the matrix to JSON
    let input_json = cost::convert_matrix_to_json(&elevators, &hall_calls);
    println!("Sending JSON: {}", input_json);



    // Serialize the input data to JSON
    let json_bytes = input_json.as_bytes();

    // Call the external executable with the JSON input
    let output = run_hra_executable(&hra_executable, json_bytes);

    // Deserialize the output JSON
    let output: std::collections::HashMap<String, Vec<[bool; 2]>> =
        serde_json::from_slice(&output).expect("Failed to deserialize output JSON");

    // Print the output
    println!("Output: ");
    for (key, value) in output {
        println!("{:6} : {:?}", key, value);
    }
}

// Function to run the external HRA executable with JSON input






/* 
        let json_data = r#"
        {
            "id_1": [[true, false], [false, true],[false, false], [false, false]],
            "id_4": [[true, true], [false, true],[false, false], [false, false]],
            "id_3": [[false, false], [false, true],[false, false], [false, false]]
        }"#;
    
        let matrix = cost::convert_json_to_matrix(json_data);
    
        // Print the result to verify
        for (i, row) in matrix.iter().enumerate() {
            println!("Row {}: {:?}", i + 1, row);
        }*/