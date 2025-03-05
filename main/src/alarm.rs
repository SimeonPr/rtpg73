use core::time::Duration;
use std::thread;

use crossbeam_channel as cbc;
use log::debug;
pub fn run(alarm_tx: cbc::Sender<u8>, timeout: Duration) {
    loop {
        debug!("Going to sleep");
        thread::sleep(timeout);
        debug!("Sending alarm");
        alarm_tx.send(0).unwrap();
    }
}
