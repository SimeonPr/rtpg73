use std::io::{self, Read, Write};
use std::net::{TcpStream, TcpListener};

fn main() -> io::Result<()> {
    // Connect to the remote stream
    let mut stream = TcpStream::connect("10.100.23.204:34933")?;

    // Bind the listener to your local address
    let listener = TcpListener::bind("0.0.0.0:35455")?;

    // Message to send to the remote stream
    let buffer = b"Connect to: 10.100.23.18:35455\0";
    
    // Send the message to the remote stream
    stream.write_all(buffer)?;

    // Prepare a buffer for receiving data
    let mut recv_buffer = [0; 512];

    // Handle incoming connections from the listener
    for incoming_stream in listener.incoming() {
        match incoming_stream {
            Ok(mut incoming_stream) => {
                // Read from the incoming connection
                let bytes_read = incoming_stream.read(&mut recv_buffer)?;
                let received_data = String::from_utf8_lossy(&recv_buffer[..bytes_read]);

                println!("Received: {}", received_data);

                // Optionally write back to the incoming stream
                incoming_stream.write_all(b"Message received")?;
                
                let bytes_read = incoming_stream.read(&mut recv_buffer)?;
                let received_data = String::from_utf8_lossy(&recv_buffer[..bytes_read]);

                println!("Received: {}", received_data);
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }

    Ok(())
}
