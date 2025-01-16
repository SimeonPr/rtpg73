use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
	{
	let buf = [4, 3, 2, 2, 1, 4, 3, 2, 2, 1];
	let socket = UdpSocket::bind("0.0.0.0:20008")?;
	socket.send_to(&buf, "10.100.23.204:20008");

	
        
    // Receives a single datagram message on the socket. If `buf` is too small to hold
    // the message, it will be cut off.
    let mut buf1 = [0; 10];
    let (amt, src) = socket.recv_from(&mut buf1)?;
    println!("Received from {}: {:?}", src, &buf1[..amt]);
	} // the socket is closed here

Ok(())
}