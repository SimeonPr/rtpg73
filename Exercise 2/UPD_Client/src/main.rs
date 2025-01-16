use std::net::UdpSocket;

fn main() {
	let buf = [4, 3, 2, 2, 1, 4, 3, 2, 2, 1];
	let socket = UdpSocket::bind("0.0.0.0:0").expect("couldn't bind to address");
	socket.send_to(&buf, "10.22.104.47:34254").expect("couldn't send data");
}