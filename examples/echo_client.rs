extern crate msgpack_rpc;
extern crate rmp as msgpack;
extern crate mioco;

use msgpack::Value;

use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use msgpack_rpc::Client;

use std::thread;

fn main() {
    let addr = "127.0.0.1:9000".parse().unwrap();

    // Setup the server socket
    let stream = mioco::mio::tcp::TcpStream::connect(&addr).unwrap();

    let mut client = Client::new(stream);

    loop {
        let result = client.call("echo", vec![Value::String("Hello, world!".to_owned())]).unwrap();
        thread::sleep_ms(1000);
        println!("{}", result);
    }
}
