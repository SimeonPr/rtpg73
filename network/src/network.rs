use crossbeam_channel as cbc;
use serde::{Serialize, Deserialize};
use bincode;

use core::time::Duration;
use std::{net::{SocketAddr, UdpSocket}, thread};

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkMessage {
    Decider(u8),
    Sender(u8),
    Receiver(u8)
}

pub fn sender(rx: cbc::Receiver<NetworkMessage>) {
    // Define the address and port to bind the socket to
    let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let destination_addr: SocketAddr = "0.0.0.0:4567".parse().unwrap();
    // Create the UDP socket
    let socket = UdpSocket::bind(addr).unwrap();
    println!("Sending on {}", socket.local_addr().unwrap());

    // Buffer to store incoming data


    // Loop to receive and process packets
    loop {
        println!("sender: Waiting for input...");
        cbc::select! {
            recv(rx) -> a => {
                let packet = a.unwrap();
                let serialized = bincode::serialize(&packet).unwrap();
                println!("sender: serialized: {:?}", serialized);
                socket.send_to(&serialized, destination_addr).unwrap();
            }
        }        
    }
    
}
pub fn receiver(decider_tx: cbc::Sender<NetworkMessage>) {
    let addr: SocketAddr = "0.0.0.0:4567".parse().unwrap();

    let socket = UdpSocket::bind(addr).unwrap();
    println!("Listening on {}", socket.local_addr().unwrap());

    let mut buf = [0u8; 1024];

    loop {
        println!("receiver: Waiting for input...");
        let (_, _) = socket.recv_from(&mut buf).unwrap();
        // Deserialize the binary data back to a struct
        let deserialized: NetworkMessage = bincode::deserialize(&buf).unwrap();
        println!("receiver: deserialized {:?}", deserialized);
        decider_tx.send(deserialized).unwrap();
    }
}
pub fn decider(rx: cbc::Receiver<NetworkMessage>, sender_tx: cbc::Sender<NetworkMessage>) {
    let mut counter = 0;
    loop {
        thread::sleep(Duration::from_secs(1));
        println!("decider: sending message");
        sender_tx.send(NetworkMessage::Sender(counter)).unwrap();

        cbc::select! {
            recv(rx) -> a => {
                let packet = a.unwrap();
                println!("decider: local({}) packet({:?})", counter, packet)
            }
        }
        counter += 1;
    }
}
