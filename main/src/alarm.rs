//! A module for handling periodic alarm notifications via a channel.
use core::time::Duration;
use std::thread;

use crossbeam_channel as cbc;
use log::debug;

/// Runs a continuous loop that sends periodic alarm notifications.
///
/// This function will:
/// 1. Sleep for the specified duration
/// 2. Send a notification (value `0`) through the provided channel
/// 3. Repeat indefinitely
///
/// # Parameters
/// - `alarm_tx`: A crossbeam channel sender used to notify receivers when the alarm triggers.
///               The sent value is always `0`.
/// - `timeout`: The duration to wait between alarm notifications.
pub fn run(alarm_tx: cbc::Sender<u8>, timeout: Duration) {
    loop {
        debug!("Going to sleep");
        thread::sleep(timeout);
        debug!("Sending alarm");
        alarm_tx.send(0).expect("send to alarm failed");
    }
}
