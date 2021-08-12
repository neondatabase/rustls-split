use std::{
    io::{Cursor, Read, Write},
    net::{TcpListener, TcpStream, Shutdown},
    sync::Arc,
};

use rustls::Session;

fn make_tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let client_stream = TcpStream::connect(addr).unwrap();
    let (server_stream, _) = listener.accept().unwrap();
    (server_stream, client_stream)
}

fn read_key() -> rustls::PrivateKey {
    let mut cursor = Cursor::new(include_bytes!("key.pem"));
    rustls::internal::pemfile::rsa_private_keys(&mut cursor).unwrap()[0].clone()
}

fn read_cert() -> rustls::Certificate {
    let mut cursor = Cursor::new(include_bytes!("cert.pem"));
    rustls::internal::pemfile::certs(&mut cursor).unwrap()[0].clone()
}

fn make_client_cfg() -> Arc<rustls::ClientConfig> {
    let mut cfg = rustls::ClientConfig::new();
    cfg.root_store.add(&read_cert()).unwrap();
    Arc::new(cfg)
}

fn make_server_cfg() -> Arc<rustls::ServerConfig> {
    let mut cfg = rustls::ServerConfig::new(rustls::NoClientAuth::new());
    cfg.set_single_cert(vec![read_cert()], read_key()).unwrap();
    Arc::new(cfg)
}

#[test]
fn e2e() {
    let (mut server_stream, mut client_stream) = make_tcp_pair();

    const ITERS: u64 = 1_000_000;
    const MSG: &[u8] = b"HELLO WORLD";

    let server_thread = std::thread::Builder::new()
        .name("server".into())
        .spawn(move || {
            let server_cfg = make_server_cfg();
            let mut session = rustls::ServerSession::new(&server_cfg);
            session.complete_io(&mut server_stream).unwrap();

            let (mut read_half, mut write_half) = rustls_split::split(server_stream, session, 8192);

            let bytes_copied = std::io::copy(&mut read_half, &mut write_half).unwrap();
            assert_eq!(bytes_copied, ITERS * MSG.len() as u64);
            write_half.shutdown(Shutdown::Write).unwrap();
        })
        .unwrap();

    let client_cfg = make_client_cfg();
    let dns = webpki::DNSNameRef::try_from_ascii_str("localhost").unwrap();
    let mut session = rustls::ClientSession::new(&client_cfg, dns);

    session.complete_io(&mut client_stream).unwrap();

    let (mut read_half, mut write_half) = rustls_split::split(client_stream, session, 8192);

    let writer_thread = std::thread::Builder::new()
        .name("writer".into())
        .spawn(move || {
            for _ in 0..ITERS {
                write_half.write_all(&MSG).unwrap();
            }

            write_half.shutdown(Shutdown::Write).unwrap();
        })
        .unwrap();

    let reader_thread = std::thread::Builder::new()
        .name("reader".into())
        .spawn(move || {
            let mut buf = vec![0u8; MSG.len()];

            for _ in 0..ITERS {
                read_half.read_exact(&mut buf).unwrap();
                assert_eq!(buf, MSG);
            }

            assert_eq!(0, read_half.read(&mut buf).unwrap());
        })
        .unwrap();

    server_thread.join().unwrap();
    reader_thread.join().unwrap();
    writer_thread.join().unwrap();
}
