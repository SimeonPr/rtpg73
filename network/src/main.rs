use std::thread::spawn;
use crossbeam_channel as cbc;
mod network;

fn main() {
    // Spawn the threads
    let (sender_tx, sender_rx) = cbc::unbounded::<network::NetworkMessage>();
    let (decider_tx, decider_rx) = cbc::unbounded::<network::NetworkMessage>();

    let sender = spawn(|| network::sender(sender_rx));
    let receiver = spawn(|| network::receiver(decider_tx));
    let decider = spawn(|| network::decider(decider_rx, sender_tx));

    sender.join().unwrap();
    receiver.join().unwrap();
    decider.join().unwrap();
}
