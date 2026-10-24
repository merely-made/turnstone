//! A short-lived, owned relay for the native co-op receipt, never product startup.
use iroh_relay::server::{Access, AccessControl, ClientRequest, RelayConfig, Server, ServerConfig};
use serde_json::json;
use std::{
    io::Write,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncBufReadExt;

fn emit(value: serde_json::Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}
fn at_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
#[derive(Debug)]
struct Observe;
impl AccessControl for Observe {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        emit(
            json!({"event":"connect", "at_ms":at_ms(), "endpoint":request.endpoint_id().to_string(),
                    "connection":format!("{:?}",request.connection_id())}),
        );
        Access::Allow
    }
}
fn snapshot(server: &Server, event: &str) {
    let m = &server.metrics().server;
    emit(
        json!({"event":event,"at_ms":at_ms(),"pid":std::process::id(),
        "bytes_sent":m.bytes_sent.get(),"bytes_recv":m.bytes_recv.get(),
        "packets_sent":m.send_packets_sent.get(),"packets_recv":m.send_packets_recv.get(),
        "accepts":m.accepts.get(),"disconnects":m.disconnects.get()}),
    );
}
#[tokio::main]
async fn main() {
    let bind: std::net::SocketAddr = std::env::args()
        .nth(1)
        .expect("BIND_IP:PORT")
        .parse()
        .unwrap();
    let mut relay = RelayConfig::new(bind);
    relay.access = Arc::new(Observe);
    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    let mut server = Server::spawn(config).await.expect("bind owned relay");
    emit(
        json!({"event":"ready","pid":std::process::id(),"url":format!("http://{}/",server.http_addr().unwrap()),"at_ms":at_ms()}),
    );
    let mut input = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            line=input.next_line()=>match line.unwrap().as_deref() {
                Some("snapshot")=>snapshot(&server,"snapshot"),
                Some("stop")|None=>break,
                _=>panic!("unknown relay control"),
            },
            failed=server.join()=>panic!("relay stopped unexpectedly: {failed:?}"),
        }
    }
    snapshot(&server, "final");
    server.shutdown().await.expect("ordinary relay shutdown");
    emit(json!({"event":"stopped","pid":std::process::id(),"at_ms":at_ms()}));
}
