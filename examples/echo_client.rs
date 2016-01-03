extern crate msgpack_rpc;
extern crate rmp as msgpack;
extern crate mioco;
extern crate env_logger;

use msgpack::Value;

use std::net::ToSocketAddrs;

use msgpack_rpc::Client;

fn main() {
    env_logger::init().unwrap();
    let mut client = Client::new();

    loop {
        let result = client.call("echo", vec![Value::String("Hello, world!".to_owned())]);
        println!("{:?}", result);
    }
}
