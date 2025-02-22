use core::time::Duration;
use std::thread;

use crossbeam_channel as cbc;
pub fn run(alarm_tx: cbc::Sender<u8>, timeout: Duration) {
    loop {
        thread::sleep(timeout);
        alarm_tx.send(0).unwrap();
    }
}
