use std::{
    io,
    io::Write,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex, MutexGuard},
};

use rustls::Connection;

mod buffer;
pub use buffer::BufCfg;
use buffer::Buffer;

struct Shared {
    stream: TcpStream,
    connection: Mutex<Connection>,
}

pub struct ReadHalf {
    shared: Arc<Shared>,
    buf: Buffer,
}

impl io::Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut connection = self.shared.connection.lock().unwrap();

        while connection.wants_read() {
            if self.buf.is_empty() {
                drop(connection);

                let bytes_read = self.buf.read_from(&mut &self.shared.stream)?;

                connection = self.shared.connection.lock().unwrap();

                if bytes_read == 0 {
                    break;
                }
            }

            let bytes_read = connection.read_tls(&mut self.buf)?;
            debug_assert_ne!(bytes_read, 0);

            connection
                .process_new_packets()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }

        match connection.reader().read(buf) {
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

impl ReadHalf {
    pub fn shutdown(&mut self, how: Shutdown) -> io::Result<()> {
        self.shared.stream.shutdown(how)
    }
}

pub struct WriteHalf {
    shared: Arc<Shared>,
    buf: Buffer,
}

impl WriteHalf {
    pub fn shutdown(&mut self, how: Shutdown) -> io::Result<()> {
        if how == Shutdown::Read {
            return self.shared.stream.shutdown(Shutdown::Read);
        }

        let mut connection = self.shared.connection.lock().unwrap();
        connection.send_close_notify();
        let res = flush(&mut self.buf, &self.shared, connection);
        self.shared.stream.shutdown(how)?;
        res
    }
}

fn wants_write_loop<'a>(
    buf: &mut Buffer,
    shared: &'a Shared,
    mut connection: MutexGuard<'a, Connection>,
) -> io::Result<MutexGuard<'a, Connection>> {
    while connection.wants_write() {
        while buf.is_full() {
            drop(connection);

            buf.write_to(&mut &shared.stream)?;

            connection = shared.connection.lock().unwrap();
        }

        connection.write_tls(buf)?;
    }

    Ok(connection)
}

fn flush<'a>(
    buf: &mut Buffer,
    shared: &'a Shared,
    mut connection: MutexGuard<'a, Connection>,
) -> io::Result<()> {
    connection.writer().flush()?;

    let connection = wants_write_loop(buf, shared, connection)?;
    std::mem::drop(connection);

    while !buf.is_empty() {
        buf.write_to(&mut &shared.stream)?;
    }

    Ok(())
}

impl io::Write for WriteHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let connection = self.shared.connection.lock().unwrap();
        let mut connection = wants_write_loop(&mut self.buf, &self.shared, connection)?;
        connection.writer().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let connection = self.shared.connection.lock().unwrap();
        flush(&mut self.buf, &self.shared, connection)
    }
}

pub fn split<D1: Into<Vec<u8>>, D2: Into<Vec<u8>>>(
    stream: TcpStream,
    connection: Connection,
    read_buf_cfg: BufCfg<D1>,
    write_buf_cfg: BufCfg<D2>,
) -> (ReadHalf, WriteHalf) {
    assert!(!connection.is_handshaking());

    let shared = Arc::new(Shared {
        stream,
        connection: Mutex::new(connection),
    });

    let read_half = ReadHalf {
        shared: shared.clone(),
        buf: Buffer::build_from(read_buf_cfg),
    };

    let write_half = WriteHalf {
        shared,
        buf: Buffer::build_from(write_buf_cfg),
    };

    (read_half, write_half)
}
