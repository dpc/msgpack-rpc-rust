extern crate msgpack_rpc;
extern crate rmp as msgpack;
extern crate mioco;
extern crate env_logger;

use msgpack::Value;

use std::net::ToSocketAddrs;

use msgpack_rpc::Client;

fn main() {
    env_logger::init().unwrap();

    // Setup the server socket
    let stream = mioco::mio::tcp::TcpStream::connect(&"localhost:9000"
                                                          .to_socket_addrs()
                                                          .unwrap()
                                                          .next()
                                                          .unwrap())
                     .unwrap();

    let mut client = Client::new(stream);

    loop {
        let result = client.call("echo", vec![Value::String("Hello, world!".to_owned())]);
        println!("{:?}", result);
    }
}
