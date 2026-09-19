//! US-47: the instance is stopped when idle and on every deploy, so the
//! server has to stop cleanly on the signal it is sent — finishing what is in
//! flight and closing the database, so SQLite's WAL is checkpointed into the
//! database file rather than left to recovery on the next boot.
//!
//! These drive the real binary, since what is under test is the process: its
//! signal handling, its exit, and the files it leaves behind.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::common::{import_request, test_salt, test_token, SAMPLE_GPX, TEST_PASSWORD};
use axum::body::to_bytes;
use trip_archive::config::storage::DB_FILENAME;

/// The server process, killed outright if a test fails before stopping it.
struct Server {
    child: Child,
    addr: SocketAddr,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// Start the binary on a free loopback port, with its data under `data_dir`,
    /// and wait until it says where it listens.
    fn start(data_dir: &Path) -> Self {
        // The salt the harness's tokens are signed under, where `main` reads
        // it from (US-55) — otherwise it generates its own and no token of
        // ours verifies.
        std::fs::write(
            data_dir.join(trip_archive::config::auth::SALT_FILENAME),
            test_salt().as_bytes(),
        )
        .expect("write the session salt");
        let mut child = Command::new(env!("CARGO_BIN_EXE_trip-archive"))
            .env("TRIP_ARCHIVE_PASSWORD", TEST_PASSWORD)
            .env("TRIP_ARCHIVE_DATA_DIR", data_dir)
            .env("TRIP_ARCHIVE_BIND_ADDR", "127.0.0.1:0")
            .env("RUST_LOG", "trip_archive=info")
            .env_remove("KOMOOT_EMAIL")
            .env_remove("KOMOOT_PASSWORD")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server starts");

        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let addr = lines
            .find_map(|line| {
                let line = line.ok()?;
                let (_, rest) = line.split_once("listening on http://")?;
                rest.split_whitespace().next()?.parse().ok()
            })
            .expect("the server reports its address");
        // Keep draining the log so the server never blocks on a full pipe.
        std::thread::spawn(move || lines.for_each(drop));
        Self { child, addr }
    }

    fn signal(&self, name: &str) {
        let status = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(self.child.id().to_string())
            .status()
            .expect("kill runs");
        assert!(status.success());
    }

    fn is_running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "the server did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A raw HTTP/1.1 import request — the head and the body separately, so a test
/// can stop sending halfway through.
async fn raw_import() -> (Vec<u8>, Vec<u8>) {
    let request = import_request(SAMPLE_GPX);
    let content_type = request.headers()["content-type"]
        .to_str()
        .unwrap()
        .to_owned();
    let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
    let head = format!(
        "POST /api/import HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n\
         Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        test_token(),
        body.len()
    );
    (head.into_bytes(), body.to_vec())
}

/// The status code of the response on `stream`, read to its end.
fn status_of(mut stream: TcpStream) -> u16 {
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("an HTTP response, got {response:?}"))
}

fn import(addr: SocketAddr, head: &[u8], body: &[u8]) -> u16 {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(head).unwrap();
    stream.write_all(body).unwrap();
    status_of(stream)
}

/// How many trips the database at `data_dir` holds, read after the server is gone.
async fn trip_count(data_dir: &Path) -> i64 {
    let pool = trip_archive::server::db::create_pool(&data_dir.join(DB_FILENAME))
        .await
        .unwrap();
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM trip")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count
}

fn wal_file(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(format!("{DB_FILENAME}-wal"))
}

#[tokio::test]
async fn us47_a_stop_signal_ends_the_server_cleanly_with_the_wal_checkpointed() {
    // SIGTERM is what the platform sends (`kill_signal` in fly.toml); SIGINT
    // is Ctrl-C on the laptop, and the platform's own default.
    for signal in ["TERM", "INT"] {
        let dir = tempfile::tempdir().unwrap();
        let mut server = Server::start(dir.path());
        let (head, body) = raw_import().await;
        assert_eq!(import(server.addr, &head, &body), 303, "SIG{signal}");
        assert!(wal_file(dir.path()).exists(), "the write went to the WAL");

        server.signal(signal);
        let status = server.wait_for_exit();

        assert!(status.success(), "SIG{signal}: exited with {status}");
        assert!(
            !wal_file(dir.path()).exists(),
            "SIG{signal}: the WAL was checkpointed and removed"
        );
        assert_eq!(trip_count(dir.path()).await, 1, "SIG{signal}");
    }
}

#[tokio::test]
async fn us47_a_request_in_flight_when_the_signal_arrives_still_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server::start(dir.path());
    let (head, body) = raw_import().await;
    let (first_half, second_half) = body.split_at(body.len() / 2);

    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream.write_all(&head).unwrap();
    stream.write_all(first_half).unwrap();
    std::thread::sleep(Duration::from_millis(200));

    server.signal("TERM");
    std::thread::sleep(Duration::from_millis(200));
    assert!(server.is_running(), "the server waits for the upload");

    stream.write_all(second_half).unwrap();
    assert_eq!(status_of(stream), 303, "the import completes");
    assert!(server.wait_for_exit().success());
    assert_eq!(trip_count(dir.path()).await, 1);
}
