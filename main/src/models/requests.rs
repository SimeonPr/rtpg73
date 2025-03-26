use std::collections::HashSet;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum RequestState {
    None,
    Unconfirmed,
    Confirmed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
    state: RequestState,
    acks: HashSet<u8>,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            state: RequestState::None,
            acks: HashSet::new(),
        }
    }
}

impl Request {
    pub fn new(id: u8) -> Request {
        let mut r = Request::default();
        r.acks.insert(id);
        r
    }

    pub fn merge(&mut self, other: &Request, id: u8) -> bool {
        let mut updated = false;

        if other.state as u8 > self.state as u8 {
            self.state = other.state;
            updated = true;
        }

        for ack in &other.acks {
            updated |= self.acks.insert(*ack);
        }

        updated |= self.acks.insert(id);
        updated
    }

    pub fn set_to(&mut self, new_state: RequestState, id: u8) {
        self.state = new_state;
        self.acks.clear();
        self.acks.insert(id);
    }

    // Getter for state
    pub fn state(&self) -> RequestState {
        self.state
    }

    // Getter for acks
    pub fn acks(&self) -> &HashSet<u8> {
        &self.acks
    }
}
