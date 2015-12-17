extern crate eventual;
extern crate rmp as msgpack;
extern crate serde;

use std::io;
use std::io::Cursor;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use eventual::Future;

use msgpack::value::{Value, Integer};
use msgpack::decode::value::read_value;
use msgpack::encode::value::write_value;

use serde::{Deserialize, Serialize};

pub struct RpcError;

pub struct Client {
    pub transport: TcpStream,
    pub id_counter: Arc<AtomicUsize>,
}

impl Client {
    pub fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, Value> {
        let request = Message::Request {
            id: self.id_counter.fetch_add(1, Ordering::Relaxed) as u32,
            method: method.to_owned(),
            params: params.to_owned(),
        };

        self.transport.write(&request.pack()).unwrap();
        let response = Message::unpack(&self.transport).unwrap();

        match response {
            Message::Response { ref result, .. } => result.to_owned(),
            _ => unimplemented!(),
        }
    }

    fn async_call(method: &str, params: Vec<Value>) -> Future<Value, RpcError> {
        let (tx, future) = Future::<Value, RpcError>::pair();

        future
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum Message {
    Request {
        id: u32,
        method: String,
        params: Vec<Value>,
    },
    Response {
        id: u32,
        result: Result<Value, Value>,
    },
    Notification {
        method: String,
        params: Vec<Value>,
    },
}

impl Message {
    pub fn msgtype(&self) -> i32 {
        match *self {
            Message::Request { .. } => 0,
            Message::Response { .. } => 1,
            Message::Notification { .. } => 2,
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

                Message::Request {
                    id: id as u32,
                    method: method.to_owned(),
                    params: params.to_owned(),
                }
            },
            1 => {
                let id = if let Value::Integer(Integer::U64(id)) = *array.get(1).unwrap() {
                    id
                } else {
                    panic!();
                };

                let err = array.get(2).unwrap().to_owned();
                let rpc_result = array.get(3).unwrap().to_owned();

                println!("{} {}", err, rpc_result);

                let result = match err {
                    Value::Nil => Ok(rpc_result),
                    _ => Err(err),
                };

                Message::Response {
                    id: id as u32,
                    result: result,
                }
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

                Message::Notification {
                    method: method.to_owned(),
                    params: params.to_owned(),
                }
            }
            _ => unimplemented!(),
        };
        Ok(message)
    }

    pub fn pack(&self) -> Vec<u8> {
        let value = match *self {
            Message::Request { id, ref method, ref params, } => {
                Value::Array(vec![
                    Value::Integer(Integer::U64(self.msgtype() as u64)),
                    Value::Integer(Integer::U64(id as u64)),
                    Value::String(method.to_owned()),
                    Value::Array(params.to_owned()),
                ])
            },
            Message::Response { id, ref result } => {
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
            Message::Notification { ref method, ref params } => {
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

    let notification = Message::Notification {
        method: "ping".to_owned(),
        params: vec![Value::String("hi".to_owned())],
    };

    let bytes = notification.pack();
    let mut cursor = Cursor::new(&bytes);
    let unpacked_notification = Message::unpack(&mut cursor).unwrap();

    assert_eq!(notification, unpacked_notification);
}
