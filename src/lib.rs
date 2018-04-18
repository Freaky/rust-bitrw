use std::io;
use std::io::SeekFrom;
use std::io::{Error, ErrorKind};

const MASKS: [u64; 8] = [0, 0b1, 0b11, 0b111, 0b1111, 0b11111, 0b111111, 0b1111111];

/// `The BitReader` struct adds bit-level reading to any io::Reader.
///
/// Most readers should probably be wrapped in a `BufReader` to avoid single-byte
/// reads.
#[derive(Debug)]
pub struct BitReader<R> {
    inner: R,
    buffer: [u8; 1],
    unused: u8,
}

impl<R: io::Read> BitReader<R> {
    /// Create a new `BitReader` around the given reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: [0],
            unused: 0,
        }
    }

    /// Reset the internal state of the BitReader. The next read will load fresh
    /// data from the current position of the reader and start from the beginning
    /// of the first byte returned.
    pub fn reset(&mut self) {
        self.buffer[0] = 0;
        self.unused = 0;
    }

    /// Read a single bit from the reader.
    pub fn read_bit(&mut self) -> io::Result<u8> {
        let bit = self.read_bits(1)?;
        Ok(bit as u8)
    }

    /// Read up to 64 bits from the reader.
    pub fn read_bits(&mut self, nbits: u8) -> io::Result<u64> {
        assert!(nbits <= 64);

        let mut ret: u64 = 0;
        let mut rbits = nbits;

        while rbits > self.unused {
            ret |= (self.buffer[0] as u64) << (rbits - self.unused);
            rbits -= self.unused;

            self.inner.read_exact(&mut self.buffer)?;

            self.unused = 8;
        }

        if rbits > 0 {
            ret |= (self.buffer[0] as u64) >> (self.unused - rbits);
            self.buffer[0] &= MASKS[(self.unused - rbits) as usize] as u8;
            self.unused -= rbits;
        }

        Ok(ret)
    }

    /// Get a reference to the reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Get a mutable reference to the reader.  Be sure to call `reset` if you
    /// changed the file position or otherwise mutated it.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Unwrap this `BitReader`, returning the underlying reader and discarding any
    /// unread buffered bits.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: io::Read + io::Seek> BitReader<R> {
    /// Seek to the given *bit* position in the file.  Currently only
    /// `SeekFrom::Start` and `SeekFrom::End` with negative offsets are supported.
    pub fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            SeekFrom::Start(pos) => {
                self.reset();
                self.inner.seek(SeekFrom::Start(pos / 8))?;
                self.read_bits((pos % 8) as u8)?;
                Ok(pos)
            }
            SeekFrom::End(pos) => {
                self.reset();
                if pos < 0 {
                    let mut bypos = pos / 8;
                    let bipos = 8 - (pos % 8);
                    if bipos > 0 {
                        bypos -= 1;
                    }
                    let ipos = self.inner.seek(SeekFrom::End(bypos))?;
                    self.read_bits(bipos as u8)?;
                    Ok(ipos + (pos % 8) as u64)
                } else {
                    Err(Error::new(
                        ErrorKind::Other,
                        "SeekFrom::End(seeking past end of file not yet supported",
                    ))
                }
            }
            SeekFrom::Current(_pos) => Err(Error::new(
                ErrorKind::Other,
                "SeekFrom::Current not yet supported",
            )),
        }
    }
}

/// The `BitWriter` struct adds bit-level writing to any io::Write.
///
/// Most writers should probably be wrapped in a `BufWriter` to avoid single-byte
/// writes.
#[derive(Debug)]
pub struct BitWriter<W> {
    inner: W,
    buffer: u64,
    unused: u64,
}

impl<W: io::Write> BitWriter<W> {
    /// Create a new `BitWriter` around the given writer.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            buffer: 0,
            unused: 8,
        }
    }

    /// Write a single bit to the writer.
    pub fn write_bit(&mut self, bit: u8) -> io::Result<()> {
        assert!(bit <= 1);
        self.write_bits(1, bit as u64)?;
        Ok(())
    }

    /// Write up to 64 bits to the writer.
    pub fn write_bits(&mut self, nbits: u8, value: u64) -> io::Result<usize> {
        assert!(nbits <= 64);

        let mut nbits_remaining = nbits as u64;

        // can we fill up a partial byte?
        if nbits_remaining >= self.unused && self.unused < 8 {
            let excess_bits = nbits_remaining - self.unused;
            self.buffer <<= self.unused;
            self.buffer |= (value >> excess_bits) & MASKS[self.unused as usize];

            self.inner.write_all(&[self.buffer as u8])?;

            nbits_remaining = excess_bits;
            self.unused = 8;
            self.buffer = 0;
        }

        // let's write while we can fill up full bytes
        while nbits_remaining >= 8 {
            nbits_remaining -= 8;
            self.inner.write_all(&[(value >> nbits_remaining) as u8])?;
        }

        // put the remaining bits in the buffer
        if nbits_remaining > 0 {
            self.buffer <<= nbits_remaining;
            self.buffer |= value & MASKS[nbits_remaining as usize];
            self.unused -= nbits_remaining;
        }
        Ok(nbits as usize)
    }

    /// Flush any pending writes to the underlying buffer, padding with zero bits
    /// up to the nearest byte if necessary, and returning the number of padding
    /// bits written.  The sum of `write_bits()` + `flush()` or `flush_bits()`
    /// will be the total number of bits delivered to the writer, and will
    /// always end on a byte boundary.
    ///
    /// This method should **always** be called prior to calling `into_inner` or
    /// before allowing the `BitWriter` to go out of scope, or buffered bytes may
    /// be lost.
    ///
    /// This also flushes the underlying writer.
    pub fn flush(&mut self) -> io::Result<usize> {
        let written = self.flush_bits()?;
        self.inner.flush()?;
        Ok(written)
    }

    /// Exactly the same as `flush()`, only it doesn't call `flush()` on the
    /// wrapped writer.
    ///
    /// This may be useful if you're going to call `into_inner()` to get at the
    /// wrapped writer in order to perform more bytewise writes, and don't care
    /// if it's all on stable storage just yet.
    pub fn flush_bits(&mut self) -> io::Result<usize> {
        if self.unused != 8 {
            self.inner.write_all(&[(self.buffer << self.unused) as u8])?;
            let written = self.unused;
            self.unused = 8;
            Ok(written as usize)
        } else {
            Ok(0)
        }
    }

    /// Get a reference to the writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Get a mutable reference to the writer.  You should `flush()` or at least
    /// `flush_bits()` prior to making any changes.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Unwrap this `BitWriter`, returning the underlying writer and discarding any
    /// unwritten buffered bits.  You should call `flush()` if this is undesirable.
    pub fn into_inner(self) -> W {
        self.inner
    }
}
