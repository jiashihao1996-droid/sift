// ============================================================
// Sift - Head-Cached Reader
// Metadata sits near the start of a photo file but takes a dozen small
// reads to walk. Reading the start once and serving those reads from
// memory keeps the per-file cost to one read; anything past it is read
// from the file as needed.
// ============================================================

use std::io::{self, Read, Seek, SeekFrom};

pub struct HeadCached<R> {
    inner: R,
    head: Vec<u8>,
    pos: u64,
}

impl<R: Read + Seek> HeadCached<R> {
    /// Read the first `head_len` bytes of `inner` (fewer if it is shorter)
    pub fn new(mut inner: R, head_len: usize) -> io::Result<Self> {
        let mut head = Vec::with_capacity(head_len);
        (&mut inner).take(head_len as u64).read_to_end(&mut head)?;
        Ok(Self {
            inner,
            head,
            pos: 0,
        })
    }
}

impl<R: Read + Seek> Read for HeadCached<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let cached = usize::try_from(self.pos)
            .ok()
            .and_then(|pos| self.head.get(pos..))
            .filter(|rest| !rest.is_empty());
        let n = match cached {
            Some(rest) => {
                let n = rest.len().min(buf.len());
                buf[..n].copy_from_slice(&rest[..n]);
                n
            }
            None => {
                self.inner.seek(SeekFrom::Start(self.pos))?;
                self.inner.read(buf)?
            }
        };
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for HeadCached<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.pos = match to {
            SeekFrom::Start(pos) => pos,
            SeekFrom::Current(delta) => self
                .pos
                .checked_add_signed(delta)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?,
            SeekFrom::End(_) => self.inner.seek(to)?,
        };
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| i as u8).collect()
    }

    fn read_at(reader: &mut HeadCached<Cursor<Vec<u8>>>, at: u64, len: usize) -> Vec<u8> {
        reader.seek(SeekFrom::Start(at)).unwrap();
        let mut buf = vec![0; len];
        reader.read_exact(&mut buf).unwrap();
        buf
    }

    #[test]
    fn reads_the_same_bytes_inside_across_and_past_the_head() {
        let data = bytes(1_000);
        let mut reader = HeadCached::new(Cursor::new(data.clone()), 100).unwrap();

        assert_eq!(read_at(&mut reader, 10, 20), data[10..30]);
        assert_eq!(read_at(&mut reader, 90, 20), data[90..110]);
        assert_eq!(read_at(&mut reader, 500, 20), data[500..520]);
    }

    #[test]
    fn reports_the_end_of_a_file_shorter_than_the_head() {
        let mut reader = HeadCached::new(Cursor::new(bytes(10)), 100).unwrap();
        reader.seek(SeekFrom::Start(8)).unwrap();
        let mut buf = [0; 4];
        assert!(reader.read_exact(&mut buf).is_err());
        assert_eq!(reader.seek(SeekFrom::End(0)).unwrap(), 10);
    }

    #[test]
    fn seeks_relative_to_the_current_position() {
        let mut reader = HeadCached::new(Cursor::new(bytes(200)), 100).unwrap();
        reader.seek(SeekFrom::Start(50)).unwrap();
        assert_eq!(reader.seek(SeekFrom::Current(-10)).unwrap(), 40);
        assert!(reader.seek(SeekFrom::Current(-100)).is_err());
    }
}
