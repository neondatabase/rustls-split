use std::io;

pub struct BufCfg<D: Into<Vec<u8>>> {
    initial_data: D,
    min_capacity: usize,
}

impl BufCfg<[u8; 0]> {
    /// Configure an empty buffer with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            initial_data: [],
            min_capacity: capacity,
        }
    }
}

impl<D: Into<Vec<u8>>> BufCfg<D> {
    /// Configure a buffer with the data in `initial_data`.
    /// The buffer capacity will be determined by the greater of `initial_data.len()` and `min_capacity`.
    pub fn with_data(initial_data: D, min_capacity: usize) -> Self {
        Self {
            initial_data,
            min_capacity,
        }
    }
}

struct Internals {
    buf: Box<[u8]>,
    start: usize,
    end: usize,
}

impl Internals {
    fn build_from<D: Into<Vec<u8>>>(cfg: BufCfg<D>) -> Self {
        let mut buf: Vec<u8> = cfg.initial_data.into();
        let end = buf.len();

        if buf.len() < cfg.min_capacity {
            // TODO ensure we're not wasting extra capacity
            buf.resize(cfg.min_capacity, 0u8);
        }

        assert_ne!(buf.len(), 0); // TODO add warnings about panics to docs

        let buf = buf.into_boxed_slice();

        Self { buf, start: 0, end }
    }

    fn is_empty(&self) -> bool {
        self.end == 0
    }

    fn is_full(&self) -> bool {
        self.end == self.buf.len()
    }

    fn advance_start(&mut self, delta: usize) {
        self.start += delta;

        if self.start == self.end {
            self.start = 0;
            self.end = 0;
        }
    }
}

pub struct ReadBuffer {
    internals: Internals,
}

impl ReadBuffer {
    pub fn build_from<D: Into<Vec<u8>>>(cfg: BufCfg<D>) -> Self {
        Self {
            internals: Internals::build_from(cfg),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.internals.is_empty()
    }

    pub fn read_from(&mut self, reader: &mut impl io::Read) -> io::Result<usize> {
        let bytes_read = reader.read(&mut self.internals.buf[self.internals.end..])?;
        self.internals.end += bytes_read;
        Ok(bytes_read)
    }
}

impl io::Read for ReadBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let src = &self.internals.buf[self.internals.start..self.internals.end];
        let len = std::cmp::min(src.len(), buf.len());
        buf[..len].copy_from_slice(&src[..len]);
        self.internals.advance_start(len);
        Ok(len)
    }
}

pub struct WriteBuffer {
    internals: Internals,
}

impl WriteBuffer {
    pub fn build_from<D: Into<Vec<u8>>>(cfg: BufCfg<D>) -> Self {
        Self {
            internals: Internals::build_from(cfg),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.internals.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.internals.is_full()
    }

    pub fn write_to(&mut self, writer: &mut impl io::Write) -> io::Result<usize> {
        let bytes_written =
            writer.write(&self.internals.buf[self.internals.start..self.internals.end])?;
        self.internals.advance_start(bytes_written);
        Ok(bytes_written)
    }
}

impl io::Write for WriteBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let dst = &mut self.internals.buf[self.internals.end..];
        let len = std::cmp::min(dst.len(), buf.len());
        dst[..len].copy_from_slice(&buf[..len]);
        self.internals.end += len;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::InvalidInput.into())
    }
}
