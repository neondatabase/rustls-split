use std::{
    io,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex, MutexGuard},
};

use rustls::Session;

mod buffer;
pub use buffer::BufCfg;
use buffer::{ReadBuffer, WriteBuffer};

struct Shared<S: Session> {
    stream: TcpStream,
    session: Mutex<S>,
}

pub struct ReadHalf<S: Session> {
    shared: Arc<Shared<S>>,
    buf: ReadBuffer,
}

impl<S: Session> io::Read for ReadHalf<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut session = self.shared.session.lock().unwrap();

        while session.wants_read() {
            if self.buf.is_empty() {
                drop(session);

                let bytes_read = self.buf.read_from(&mut &self.shared.stream)?;

                session = self.shared.session.lock().unwrap();

                if bytes_read == 0 {
                    break;
                }
            }

            let bytes_read = session.read_tls(&mut self.buf)?;
            debug_assert_ne!(bytes_read, 0);

            session
                .process_new_packets()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }

        match session.read(buf) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "TLS connection closed improperly",
            )),
            ok @ Ok(_) => ok,
            Err(ref e) if e.kind() == io::ErrorKind::ConnectionAborted => Ok(0),
            err @ Err(_) => err,
        }
    }
}

impl<S: Session> ReadHalf<S> {
    pub fn shutdown(&mut self, how: Shutdown) -> io::Result<()> {
        self.shared.stream.shutdown(how)
    }
}

pub struct WriteHalf<S: Session> {
    shared: Arc<Shared<S>>,
    buf: WriteBuffer,
}

impl<S: Session> WriteHalf<S> {
    pub fn shutdown(&mut self, how: Shutdown) -> io::Result<()> {
        if how == Shutdown::Read {
            return self.shared.stream.shutdown(Shutdown::Read);
        }

        let mut session = self.shared.session.lock().unwrap();
        session.send_close_notify();
        let res = flush(&mut self.buf, &self.shared, session);
        self.shared.stream.shutdown(how)?;
        res
    }
}

fn wants_write_loop<'a, S: Session>(
    buf: &mut WriteBuffer,
    shared: &'a Shared<S>,
    mut session: MutexGuard<'a, S>,
) -> io::Result<MutexGuard<'a, S>> {
    while session.wants_write() {
        while buf.is_full() {
            drop(session);

            buf.write_to(&mut &shared.stream)?;

            session = shared.session.lock().unwrap();
        }

        session.write_tls(buf)?;
    }

    Ok(session)
}

fn flush<'a, S: Session>(
    buf: &mut WriteBuffer,
    shared: &'a Shared<S>,
    mut session: MutexGuard<'a, S>,
) -> io::Result<()> {
    session.flush()?;

    let session = wants_write_loop(buf, shared, session)?;
    std::mem::drop(session);

    while !buf.is_empty() {
        buf.write_to(&mut &shared.stream)?;
    }

    Ok(())
}

impl<S: Session> io::Write for WriteHalf<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let session = self.shared.session.lock().unwrap();
        let mut session = wants_write_loop(&mut self.buf, &self.shared, session)?;
        session.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let session = self.shared.session.lock().unwrap();
        flush(&mut self.buf, &self.shared, session)
    }
}

pub fn split<S: Session, D1: Into<Vec<u8>>, D2: Into<Vec<u8>>>(
    stream: TcpStream,
    session: S,
    read_buf_cfg: BufCfg<D1>,
    write_buf_cfg: BufCfg<D2>,
) -> (ReadHalf<S>, WriteHalf<S>) {
    assert!(!session.is_handshaking());

    let shared = Arc::new(Shared {
        stream,
        session: Mutex::new(session),
    });

    let read_half = ReadHalf {
        shared: shared.clone(),
        buf: ReadBuffer::build_from(read_buf_cfg),
    };

    let write_half = WriteHalf {
        shared,
        buf: WriteBuffer::build_from(write_buf_cfg),
    };

    (read_half, write_half)
}
