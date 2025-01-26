use std::time;

pub struct Timer {
    timer_end: Option<u64>,
    timer_active: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            timer_end: None,
            timer_active: false,
        }
    }

    fn get_wall_time() -> u64 {
        time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)            
            .unwrap()
            .as_secs()
            .try_into()
            .unwrap_or(u64::MAX) // Handle overflow gracefully
    }

    // Start the timer with a specified duration
    pub fn start(&mut self, duration: u16) {
        self.timer_end = Some(Self::get_wall_time() + duration as u64);
        self.timer_active = true;
    }

    // Stop the timer
    pub fn stop(&mut self) {
        self.timer_active = false;
        self.timer_end = None;
    }

    // Check if the timer has timed out
    pub fn timed_out(&self) -> bool {
        if self.timer_active {
            if let Some(end_time) = self.timer_end {
                return Self::get_wall_time() >= end_time;
            }
        }
        false
    }
}
