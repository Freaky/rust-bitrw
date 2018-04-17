### bitrw

A Rust crate for bit-level IO.

## Synopsis

    extern crate bitrw;
    use bitrw::{BitReader, BitWriter};

    // Something implementing io::Write
  	let buf: Vec<u8> = Vec::new();

    let mut writer = BitWriter::new(BufWriter::new(buf);

    let mut bits_written = writer.write_bit(0)?;
    bits_written += writer.write_bits(7, 0b100_0001)?;
    bits_written += writer.write_bits(2, 0b01)?;
    bits_written += writer.flush()?; // returns u64 indicating the number of bits of padding.

    assert_eq!(buf, [0b0100_0001, 0b0100_0000]);

    // Or io::Read, and optionally io::Seek for seeking by bit position.
    let mut reader = BitReader::new(Cursor::new(writer.into_inner()));

    let bit = reader.read_bit()?; // u8, 0 or 1
    let rest_of_byte = reader.read_bits(7)?; // u64 of remainder.
    let bits = reader.read_bits(2)?;

    assert_eq!(bit, 0);
    assert_eq!(rest_of_byte, 0b100_0001);
    assert_eq!(bits, 0b01);


Currently the interface can be considered unstable.
