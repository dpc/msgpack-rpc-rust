extern crate mioco;
extern crate rmp as msgpack;
extern crate rmp_serde;
extern crate serde;

use std::io;
use std::io::Cursor;
use std::io::prelude::*;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

use std::thread;

use msgpack::value::{Value, Integer};
use msgpack::decode::value::read_value;
use msgpack::encode::value::write_value;

use mioco::mio::*;

type MessageId = i32;

pub struct ClientHandler(pub mioco::mio::tcp::TcpStream);

const SERVER: mioco::mio::Token = mioco::mio::Token(0);

impl mioco::mio::Handler for ClientHandler {
    type Message = Message;
    type Timeout = ();

    fn ready(&mut self, event_loop: &mut mioco::mio::EventLoop<Self>, token: mioco::mio::Token, _: mioco::mio::EventSet) {
        match token {
            SERVER => {
                let ClientHandler(ref mut server) = *self;

                let mut buf = vec![];

                server.read(&mut buf).unwrap();
                println!("{:?}", buf);
            }
            _ => panic!("unexpected token"),
        }
    }
}

pub struct Client {
    scheduler: mioco::MailboxOuterEnd<Message>,
    request_map: Arc<RwLock<HashMap<MessageId, mioco::MailboxOuterEnd<Result<Value, Value>>>>>,
    id_generator: AtomicUsize,
}

impl Client {
    pub fn new(transport: mioco::mio::tcp::TcpStream) -> Client {
        use std::io::prelude::*;
        let (sender, receiver) = mioco::mailbox::<Message>();

        let request_map = Arc::new(RwLock::new(HashMap::new()));

        let local_request_map = request_map.clone();
        thread::spawn(move || {
            mioco::start(move || {
                let receiver = mioco::wrap(receiver);
                loop {
                    let message = receiver.read();
                    let transport = try!(transport.try_clone());
                    let request_map = local_request_map.clone();
                    mioco::spawn(move || {
                        let mut transport = mioco::wrap(transport);
                        try!(transport.write_all(&message.pack()));

                        let mut buf = vec![];
                        try!(transport.read(&mut buf));
                        let message = try!(Message::unpack(Cursor::new(buf)));

                        match message {
                            Message::Response(Response { id, result }) => {
                                let request_map = request_map.clone();
                                let mut request_map_lock = request_map.write().unwrap();
                                let sender: mioco::MailboxOuterEnd<Result<Value, Value>> = request_map_lock.remove(&id).unwrap();
                                sender.send(result);
                            },
                            _ => unimplemented!()
                        }

                        Ok(())
                    });
                }
            });
        });

        Client {
            scheduler: sender,
            request_map: request_map,
            id_generator: AtomicUsize::new(0),
        }
    }
    // pub fn new<T>(transport: T) -> Client where T: Read + Write + Evented + Send + 'static {
    //     let mut event_loop = EventLoop::new().unwrap();
    //     let sender = event_loop.channel();

    //     thread::spawn(move || {
    //         let _ = event_loop.run(&mut ClientHandler {
    //             transport: transport,
    //             requests: Slab::new_starting_at(mio::Token(0), std::u32::MAX as usize),
    //         });
    //     });

    //     Client {
    //         scheduler: sender,
    //     }
    // }

    // fn async_call(&mut self, method: &str, params: Vec<Value>) -> Receiver<Result<Value, Value>> {
    //     let request = Request {
    //         id: 0,  // FIXME
    //         method: method.to_owned(),
    //         params: params.clone(),
    //     };

    //     let (tx, rx) = mpsc::channel();
    //     self.scheduler.send(Message::Request(request)).unwrap();

    //     rx
    // }

    fn next_id(&mut self) -> MessageId {
        let ordering = Ordering::Relaxed;

        let id: MessageId = self.id_generator.fetch_add(1, ordering) as MessageId;

        if id == std::i32::MAX as MessageId {
            self.id_generator.store(0, ordering);
        }

        id
    }

    pub fn async_call(&mut self, method: &str, params: Vec<Value>) -> mioco::MailboxInnerEnd<Result<Value, Value>> {
        let (tx, rx) = mioco::mailbox();

        let request = Request {
            id: self.next_id(),
            method: method.to_owned(),
            params: params.clone(),
        };

        let request_map = self.request_map.clone();
        let mut request_map_lock = request_map.write().unwrap();
        request_map_lock.insert(request.id, tx);

        self.scheduler.send(Message::Request(request));

        rx
    }

    pub fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, Value> {
        let mailbox = self.async_call(method, params);
        mioco::wrap(mailbox).read()
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
            },
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
            },
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
            },
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
            },
            Message::Notification(Notification { ref method, ref params }) => {
                Value::Array(vec![
                    Value::Integer(Integer::U64(self.msgtype() as u64)),
                    Value::String(method.to_owned()),
                    Value::Array(params.to_owned()),
                ])
            },
        };
        let mut bytes = vec![];
        write_value(&mut bytes, &value).unwrap();
        bytes
    }
}

#[test]
fn pack_unpack_request() {
    use std::io::Cursor;

    let request = Message::Request {
        id: 0,
        method: "echo".to_owned(),
        params: vec![Value::String("hello world!".to_owned())],
    };

    let bytes = request.pack();
    let mut cursor = Cursor::new(&bytes);
    let unpacked_request = Message::unpack(&mut cursor).unwrap();

    assert_eq!(request, unpacked_request);
}

#[test]
fn pack_unpack_response() {
    use std::io::Cursor;

    let response = Message::Response {
        id: 0,
        result: Ok(Value::String("test".to_owned())),
    };

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
