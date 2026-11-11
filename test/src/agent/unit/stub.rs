use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub line: String,
    pub authorization: Option<String>,
    pub body: String,
}

pub type Seen = Arc<Mutex<Vec<Request>>>;

pub fn serve(responses: Vec<(u16, Vec<u8>)>) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for ((status, body), stream) in responses.into_iter().zip(listener.incoming()) {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = Request::default();
            reader.read_line(&mut request.line).unwrap();
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                let (name, value) = header.split_once(": ").unwrap();
                match name.to_ascii_lowercase().as_str() {
                    "content-length" => length = value.parse().unwrap(),
                    "authorization" => request.authorization = Some(value.to_owned()),
                    _ => {}
                }
            }
            let mut buf = vec![0; length];
            reader.read_exact(&mut buf).unwrap();
            request.body = String::from_utf8(buf).unwrap();
            log.lock().unwrap().push(request);
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (base, seen)
}

pub fn ok(body: serde_json::Value) -> (u16, Vec<u8>) {
    (200, body.to_string().into_bytes())
}

pub fn content(text: &str) -> (u16, Vec<u8>) {
    ok(serde_json::json!({"choices": [{"message": {"content": text}}]}))
}

pub fn refused() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    drop(listener);
    base
}
