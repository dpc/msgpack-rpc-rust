extern crate msgpack_rpc;
extern crate rmp as msgpack;

use msgpack::Value;

use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use msgpack_rpc::Client;

fn main() {
    let mut client = Client {
        transport: TcpStream::connect(("localhost", 9000)).unwrap(),
        id_counter: Arc::new(AtomicUsize::new(0)),
    };

    let result = client.call("echo", vec![Box::new("Hello, world!")]).unwrap();
    println!("{}", result);
}
