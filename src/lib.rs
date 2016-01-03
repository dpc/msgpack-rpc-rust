#![feature(rustc_private)]

extern crate mioco;
extern crate rmp as msgpack;
extern crate rmp_serde;
extern crate serde;

#[macro_use]
extern crate log;

use std::io;
use std::io::prelude::*;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

use std::thread;

use msgpack::value::{Value, Integer};
use msgpack::decode::value::read_value;
use msgpack::encode::value::write_value;

use mioco::mio::*;

type MessageId = i32;

pub struct Client {
    sender: mioco::MailboxOuterEnd<Message>,
    request_map: Arc<Mutex<HashMap<MessageId, mpsc::Sender<Result<Value, Value>>>>>,
    id_generator: AtomicUsize,
}

impl Client {
    pub fn new() -> Client {
        use std::io::prelude::*;
        let (sender, receiver) = mioco::mailbox();

        let request_map = Arc::new(Mutex::new(HashMap::new()));

        let local_request_map = request_map.clone();
        thread::spawn(move || {
            mioco::start(move || {

                use std::net::ToSocketAddrs;
                // Setup the server socket
                let transport = mioco::mio::tcp::TcpStream::connect(&"localhost:9000"
                                                                 .to_socket_addrs()
                                                                 .unwrap()
                                                                 .next()
                                                                 .unwrap())
                    .unwrap();


                let receiver = mioco::wrap(receiver);
                loop {
                    info!("Waiting for request");
                    let request = receiver.read();
                    info!("Got request {:?}", request);
                    let request = match request {
                        Message::Request(request) => request,
                        _ => unimplemented!(),
                    };

                    let transport = try!(transport.try_clone());
                    let request_map = local_request_map.clone();
                    mioco::spawn(move || {
                        let mut transport = mioco::wrap(try!(transport.try_clone()));

                        try!(transport.write_all(&Message::Request(request).pack()));
                        let message = try!(Message::unpack(transport));

                        match message {
                            Message::Response(Response { id, result }) => {
                                let sender: mpsc::Sender<Result<Value, Value>> =
                                    request_map.lock().unwrap().remove(&id).unwrap();
                                sender.send(result).unwrap();
                            }
                            _ => unimplemented!(),
                        }

                        Ok(())
                    });

                }
            })
        });

        Client {
            sender: sender,
            request_map: request_map,
            id_generator: AtomicUsize::new(0),
        }
    }

    fn next_id(&mut self) -> MessageId {
        let ordering = Ordering::Relaxed;

        let id: MessageId = self.id_generator.fetch_add(1, ordering) as MessageId;

        if id == std::i32::MAX as MessageId {
            self.id_generator.store(0, ordering);
        }

        id
    }

    pub fn async_call(&mut self,
                      method: &str,
                      params: Vec<Value>)
                      -> mpsc::Receiver<Result<Value, Value>> {
        let (tx, rx) = mpsc::channel();

        let request = Request {
            id: self.next_id(),
            method: method.to_owned(),
            params: params.clone(),
        };

        self.request_map.lock().unwrap().insert(request.id, tx);
        self.sender.send(Message::Request(request));

        rx
    }

    pub fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, Value> {
        let receiver = self.async_call(method, params);
        receiver.recv().unwrap()
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct Request {
    id: i32,
    method: String,
    params: Vec<Value>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct Response {
    id: i32,
    result: Result<Value, Value>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct Notification {
    method: String,
    params: Vec<Value>,
}

#[derive(PartialEq, Clone, Debug)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl Request {
    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn method(&self) -> &str {
        "hi"
    }

    pub fn params(&self) -> Vec<Value> {
        vec![]
    }
}

impl Message {
    pub fn msgtype(&self) -> i32 {
        use Message::*;

        match *self {
            Request(..) => 0,
            Response(..) => 1,
            Notification(..) => 2,
        }
    }

    pub fn unpack<R>(mut reader: R) -> io::Result<Message>
        where R: Read
    {
        let value = read_value(&mut reader).unwrap();

        let array = match value {
            Value::Array(array) => array,
            _ => panic!(),
        };

        let msg_type = match *array.get(0).unwrap() {
            Value::Integer(Integer::U64(msg_type)) => msg_type,
            _ => panic!(),
        };

        let message = match msg_type {
            0 => {
                let id = if let Value::Integer(Integer::U64(id)) = *array.get(1).unwrap() {
                    id
                } else {
                    panic!();
                };

                let method = if let Value::String(ref method) = *array.get(2).unwrap() {
                    method
                } else {
                    panic!();
                };

                let params = if let Value::Array(ref params) = *array.get(3).unwrap() {
                    params
                } else {
                    panic!();
                };

                Message::Request(Request {
                    id: id as i32,
                    method: method.to_owned(),
                    params: params.to_owned(),
                })
            }
            1 => {
                let id = if let Value::Integer(Integer::U64(id)) = *array.get(1).unwrap() {
                    id
                } else {
                    panic!();
                };

                let err = array.get(2).unwrap().to_owned();
                let rpc_result = array.get(3).unwrap().to_owned();

                let result = match err {
                    Value::Nil => Ok(rpc_result),
                    _ => Err(err),
                };

                Message::Response(Response {
                    id: id as i32,
                    result: result,
                })
            }
            2 => {
                let method = if let Value::String(ref method) = *array.get(1).unwrap() {
                    method
                } else {
                    panic!();
                };

                let params = if let Value::Array(ref params) = *array.get(2).unwrap() {
                    params
                } else {
                    panic!();
                };

                Message::Notification(Notification {
                    method: method.to_owned(),
                    params: params.to_owned(),
                })
            }
            _ => unimplemented!(),
        };
        Ok(message)
    }

    pub fn pack(&self) -> Vec<u8> {
        let value = match *self {
            Message::Request(Request { id, ref method, ref params, }) => {
                Value::Array(vec![
                    Value::Integer(Integer::U64(self.msgtype() as u64)),
                    Value::Integer(Integer::U64(id as u64)),
                    Value::String(method.to_owned()),
                    Value::Array(params.to_owned()),
                ])
            }
            Message::Response(Response { id, ref result }) => {
                let (error, result) = match *result {
                    Ok(ref result) => (Value::Nil, result.to_owned()),
                    Err(ref err) => (err.to_owned(), Value::Nil),
                };

                Value::Array(vec![
                    Value::Integer(Integer::U64(self.msgtype() as u64)),
                    Value::Integer(Integer::U64(id as u64)),
                    error,
                    result,
                ])
            }
            Message::Notification(Notification { ref method, ref params }) => {
                Value::Array(vec![
                    Value::Integer(Integer::U64(self.msgtype() as u64)),
                    Value::String(method.to_owned()),
                    Value::Array(params.to_owned()),
                ])
            }
        };
        let mut bytes = vec![];
        write_value(&mut bytes, &value).unwrap();
        bytes
    }
}

#[test]
fn pack_unpack_request() {
    use std::io::Cursor;

    let request = Message::Request(Request {
        id: 0,
        method: "echo".to_owned(),
        params: vec![Value::String("hello world!".to_owned())],
    });

    let bytes = request.pack();
    let mut cursor = Cursor::new(&bytes);
    let unpacked_request = Message::unpack(&mut cursor).unwrap();

    assert_eq!(request, unpacked_request);
}

#[test]
fn pack_unpack_response() {
    use std::io::Cursor;

    let response = Message::Response(Response {
        id: 0,
        result: Ok(Value::String("test".to_owned())),
    });

    let bytes = response.pack();
    let mut cursor = Cursor::new(&bytes);
    let unpacked_response = Message::unpack(&mut cursor).unwrap();

    assert_eq!(response, unpacked_response);
}

#[test]
fn pack_unpack_notification() {
    use std::io::Cursor;

    let notification = Message::Notification(Notification {
        method: "ping".to_owned(),
        params: vec![Value::String("hi".to_owned())],
    });

    let bytes = notification.pack();
    let mut cursor = Cursor::new(&bytes);
    let unpacked_notification = Message::unpack(&mut cursor).unwrap();

    assert_eq!(notification, unpacked_notification);
}
