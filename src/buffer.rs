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

pub struct Buffer {
    buf: Box<[u8]>,
    start: usize,
    end: usize,
}

impl Buffer {
    pub fn build_from<D: Into<Vec<u8>>>(cfg: BufCfg<D>) -> Self {
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

    pub fn read_from(&mut self, reader: &mut impl io::Read) -> io::Result<usize> {
        let bytes_read = reader.read(&mut self.buf[self.end..])?;
        self.end += bytes_read;
        Ok(bytes_read)
    }

    pub fn write_to(&mut self, writer: &mut impl io::Write) -> io::Result<usize> {
        let bytes_written = writer.write(&self.buf[self.start..self.end])?;
        self.start += bytes_written;
        self.check_start();
        Ok(bytes_written)
    }

    pub fn is_empty(&self) -> bool {
        self.end == 0
    }

    pub fn is_full(&self) -> bool {
        self.end == self.buf.len()
    }

    fn check_start(&mut self) {
        if self.start == self.end {
            self.start = 0;
            self.end = 0;
        }
    }
}

impl io::Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let src = &self.buf[self.start..self.end];
        let len = std::cmp::min(src.len(), buf.len());
        buf[..len].copy_from_slice(&src[..len]);
        self.start += len;
        self.check_start();
        Ok(len)
    }
}

impl io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let dst = &mut self.buf[self.end..];
        let len = std::cmp::min(dst.len(), buf.len());
        dst[..len].copy_from_slice(&buf[..len]);
        self.end += len;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::InvalidInput.into())
    }
}
