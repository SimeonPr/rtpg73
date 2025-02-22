use core::time::Duration;
use std::thread;

use crossbeam_channel as cbc;
pub fn run(alarm_tx: cbc::Sender<u8>) {
    loop {
        thread::sleep(Duration::from_secs(1));
        alarm_tx.send(0).unwrap();
    }
}
