use core::time::Duration;
use std::fs::{self, OpenOptions};
use std::process::Command;
use std::thread::{self, sleep};
use std::time::SystemTime;
use std::path::Path;
use std::io::{prelude::*, SeekFrom};

const TIMEOUT_SECONDS: u64 = 2; // when should the actor be considered crashed
const COMMUNICATION_FILENAME: &str = "counter";


fn main() {
    // ensure file exists
    let _ = OpenOptions::new().create(true).write(true).open(Path::new(COMMUNICATION_FILENAME)).expect("couldn't create file");
    
    supervisor();
}
fn supervisor() {
    let mut counter: u8 = 0;    
    let path = Path::new(COMMUNICATION_FILENAME);
    let display = path.display();

    let mut file = OpenOptions::new().read(true).open(path).expect("couldn't open file");

    loop {
        // when was the last time the file was written?

        let metadata = fs::metadata(path).expect("couldn't get metadata");

        let time = metadata.modified().expect("couldn't get modification time");

        let now = SystemTime::now();
        let duration = now.duration_since(time).expect("couldn't get elapsed time");

        if duration > Duration::from_secs(TIMEOUT_SECONDS) {         // Too long ago
            println!("Actor might be down. Restarting from last knwon state...");

            Command::new("setsid")
                .arg("alacritty")
                .arg("--command")
                .arg("bash")
                .arg("-c")
                .arg("sleep 1; cargo run -r")  
                .spawn()
                .expect("couldn't start process");

            actor(counter);
            
        } else {   // Last update was within allowed time span
            file.seek(SeekFrom::Start(0)).expect("couldn't seek file");
            
            let mut s = String::new();
            match file.read_to_string(&mut s) {
                Err(err) => println!("couldn't read {}: {}", display, err),
                Ok(_) => ()
            }

            counter = match s.parse::<u8>() {
                Err(err) => {
                    println!("couldn't parse: {} {}", err, s);
                    0
                },
                Ok(val) => val
            };
            println!("Actor seems to be alive. Current value is {}", counter);            

        }        

        sleep(Duration::from_secs(TIMEOUT_SECONDS));
    }
}

fn actor(val: u8) {
    println!("Actor started with value {val}");
    let mut val = val;
    let path = Path::new(COMMUNICATION_FILENAME);

    loop {
        // increment number
        val += 1;

        let mut file = OpenOptions::new()
            .write(true)   
            .truncate(true)
            .create(true)  
            .open(path)
            .expect("couldn't open file");
        
        file.write_all(&val.to_string().as_bytes()).expect("couldn't write file");

        println!("Current Number: {val}");
        thread::sleep(Duration::from_millis(200));
        
    }
}
