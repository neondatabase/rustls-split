use std::io;
pub struct Buffer {
    buf: Box<[u8]>,
    start: usize,
    end: usize,
}

impl Buffer {
    pub fn new(size: usize) -> Self {
        assert_ne!(size, 0);
        Self {
            buf: vec![0u8; size].into_boxed_slice(),
            start: 0,
            end: 0,
        }
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
