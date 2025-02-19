use serde::{Serialize, Deserialize};
use bincode;
#[derive(Debug, Serialize, Deserialize)]
pub enum Manager {
    Ping
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Network {
    Ping
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Controller {
    Ping
}
