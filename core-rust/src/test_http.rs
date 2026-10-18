//! Bounded local model fixture. Never contacts a public model or reads credentials.
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
pub fn server(replies: Vec<Value>, delay_ms: u64) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in replies {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("mock model accept: {e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 8192];
            let start = loop {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let head = String::from_utf8_lossy(&bytes[..start]).to_lowercase();
            let len: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            while bytes.len() < start + len {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            requests.push(serde_json::from_slice(&bytes[start..start + len]).unwrap());
            thread::sleep(Duration::from_millis(delay_ms));
            let body = reply.to_string();
            let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
        }
        requests
    });
    (url, handle)
}
pub fn config(db: &std::path::Path, url: &str) {
    rusqlite::Connection::open(db).unwrap().execute("INSERT OR REPLACE INTO mobile_ai_config(id,base_url,model,api_key,updated_at,vision) VALUES(1,?1,'fixture-model','','2026-10-18',0)",[url]).unwrap();
}
