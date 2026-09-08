use super::*;
use crate::{GitdConfig, RepoId};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEST_OBJECT_SIZE: u64 = 8 * 1024 * 1024;

#[test]
fn production_socket_streams_lfs_download_and_preserves_hash() {
    let root = temp_dir("jeryu-lfs-download-root");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "lfs-stream").expect("valid repo id"))
        .expect("create bare repository");
    let oid = repeated_oid(b'l', TEST_OBJECT_SIZE);
    LfsStore::for_repo(&repo.path)
        .put_reader_with_limit(
            &oid,
            Some(TEST_OBJECT_SIZE),
            TEST_OBJECT_SIZE,
            RepeatingReader {
                remaining: TEST_OBJECT_SIZE,
                byte: b'l',
                max_chunk: 113,
            },
        )
        .expect("store generated LFS object");

    let (address, stop, server_thread) = start_test_server(SmartHttpServer::new(manager));
    let mut client = TcpStream::connect(&address).expect("connect LFS client");
    let request = format!(
        "GET /acme/lfs-stream.git/info/lfs/objects/{oid} HTTP/1.1\r\nHost: localhost\r\n\r\n"
    );
    client.write_all(request.as_bytes()).expect("write LFS GET");
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("finish LFS GET");
    let header = read_response_head(&mut client);
    let header_text = String::from_utf8(header).expect("response head is UTF-8");
    assert!(header_text.starts_with("HTTP/1.1 200"), "{header_text}");
    assert!(
        header_text.contains(&format!("Content-Length: {TEST_OBJECT_SIZE}\r\n")),
        "{header_text}"
    );
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = client.read(&mut buffer).expect("read streamed LFS body");
        if read == 0 {
            break;
        }
        received += read as u64;
        hasher.update(&buffer[..read]);
    }
    assert_eq!(received, TEST_OBJECT_SIZE);
    assert_eq!(finalize_hex(hasher), oid);

    let missing_oid = "0".repeat(64);
    let mut missing = TcpStream::connect(&address).expect("connect missing LFS client");
    let request = format!(
        "GET /acme/lfs-stream.git/info/lfs/objects/{missing_oid} HTTP/1.1\r\nHost: localhost\r\n\r\n"
    );
    missing
        .write_all(request.as_bytes())
        .expect("write missing LFS GET");
    missing
        .shutdown(std::net::Shutdown::Write)
        .expect("finish missing LFS GET");
    let mut response = String::new();
    missing
        .read_to_string(&mut response)
        .expect("read missing response");
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");

    stop.store(true, Ordering::Release);
    server_thread.join().expect("server thread joins");
    let _ = std::fs::remove_dir_all(root);
}

fn read_response_head(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        assert!(header.len() < 64 * 1024, "response head exceeded limit");
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).expect("read response head");
        header.push(byte[0]);
    }
    header
}

fn repeated_oid(byte: u8, size: u64) -> String {
    let mut hasher = Sha256::new();
    let chunk = [byte; 64 * 1024];
    let mut remaining = size;
    while remaining > 0 {
        let count = remaining.min(chunk.len() as u64) as usize;
        hasher.update(&chunk[..count]);
        remaining -= count as u64;
    }
    finalize_hex(hasher)
}

fn finalize_hex(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

struct RepeatingReader {
    remaining: u64,
    byte: u8,
    max_chunk: usize,
}

impl Read for RepeatingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = (self.remaining as usize)
            .min(buffer.len())
            .min(self.max_chunk);
        buffer[..count].fill(self.byte);
        self.remaining -= count as u64;
        Ok(count)
    }
}

fn start_test_server(server: SmartHttpServer) -> (String, Arc<AtomicBool>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    listener
        .set_nonblocking(true)
        .expect("set listener nonblocking");
    let address = listener.local_addr().expect("listener address");
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => server
                    .handle_stream(stream)
                    .unwrap_or_else(|err| panic!("test server request failed: {err}")),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(err) => panic!("test listener failed: {err}"),
            }
        }
    });
    (address.to_string(), stop, handle)
}

fn temp_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{prefix}-{stamp}-{serial}"));
    std::fs::create_dir_all(&path).expect("create test directory");
    path
}
