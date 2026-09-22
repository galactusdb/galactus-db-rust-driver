//! PackStream â€” Neo4j's binary serialization format, used by Bolt.
//!
//! This is a faithful, std-only implementation of the subset Bolt needs: the
//! primitive types plus lists, maps and *structures* (a tagged tuple, used for
//! every Bolt message and for graph types like Node). Reference:
//! <https://neo4j.com/docs/bolt/current/packstream/>.
//!
//! Marker bytes (the ones we emit/parse):
//! ```text
//!   C0 null      C2/C3 false/true      C1 float64
//!   C8 int8  C9 int16  CA int32  CB int64   (and tiny ints -16..127 inline)
//!   80..8F tiny str    D0/D1/D2 str8/16/32
//!   90..9F tiny list   D4/D5/D6 list8/16/32
//!   A0..AF tiny map    D8/D9/DA map8/16/32
//!   B0..BF tiny struct (low nibble = field count), followed by a signature byte
//!   CC/CD/CE bytes8/16/32
//! ```

/// A PackStream value.
#[derive(Debug, Clone, PartialEq)]
pub enum Ps {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Ps>),
    /// Ordered key/value pairs (Bolt maps preserve insertion order on the wire).
    Map(Vec<(String, Ps)>),
    /// A tagged structure: a signature byte plus its fields. Bolt messages and
    /// graph types are all structures.
    Struct(u8, Vec<Ps>),
}

// ---- encoding ----------------------------------------------------------

/// Encode a value, appending its bytes to `out`.
pub fn encode(value: &Ps, out: &mut Vec<u8>) {
    match value {
        Ps::Null => out.push(0xC0),
        Ps::Bool(false) => out.push(0xC2),
        Ps::Bool(true) => out.push(0xC3),
        Ps::Int(i) => encode_int(*i, out),
        Ps::Float(f) => {
            out.push(0xC1);
            out.extend_from_slice(&f.to_be_bytes());
        }
        Ps::String(s) => {
            let b = s.as_bytes();
            match b.len() {
                n if n <= 15 => out.push(0x80 | n as u8),
                n if n <= 0xFF => {
                    out.push(0xD0);
                    out.push(n as u8);
                }
                n if n <= 0xFFFF => {
                    out.push(0xD1);
                    out.extend_from_slice(&(n as u16).to_be_bytes());
                }
                n => {
                    out.push(0xD2);
                    out.extend_from_slice(&(n as u32).to_be_bytes());
                }
            }
            out.extend_from_slice(b);
        }
        Ps::Bytes(b) => {
            match b.len() {
                n if n <= 0xFF => {
                    out.push(0xCC);
                    out.push(n as u8);
                }
                n if n <= 0xFFFF => {
                    out.push(0xCD);
                    out.extend_from_slice(&(n as u16).to_be_bytes());
                }
                n => {
                    out.push(0xCE);
                    out.extend_from_slice(&(n as u32).to_be_bytes());
                }
            }
            out.extend_from_slice(b);
        }
        Ps::List(items) => {
            encode_header(items.len(), 0x90, 0xD4, out);
            for it in items {
                encode(it, out);
            }
        }
        Ps::Map(entries) => {
            encode_header(entries.len(), 0xA0, 0xD8, out);
            for (k, v) in entries {
                encode(&Ps::String(k.clone()), out);
                encode(v, out);
            }
        }
        Ps::Struct(sig, fields) => {
            // Structures are always tiny in Bolt (<= 15 fields).
            out.push(0xB0 | (fields.len() as u8 & 0x0F));
            out.push(*sig);
            for f in fields {
                encode(f, out);
            }
        }
    }
}

/// Emit a length header for lists/maps, choosing tiny / 8 / 16 / 32 form.
/// `tiny_base` is 0x90 (list) or 0xA0 (map); `big_base` is 0xD4 / 0xD8.
fn encode_header(len: usize, tiny_base: u8, big_base: u8, out: &mut Vec<u8>) {
    match len {
        n if n <= 15 => out.push(tiny_base | n as u8),
        n if n <= 0xFF => {
            out.push(big_base);
            out.push(n as u8);
        }
        n if n <= 0xFFFF => {
            out.push(big_base + 1);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(big_base + 2);
            out.extend_from_slice(&(n as u32).to_be_bytes());
        }
    }
}

/// Encode an integer in the smallest form that fits.
fn encode_int(i: i64, out: &mut Vec<u8>) {
    if (-16..=127).contains(&i) {
        out.push((i as i8) as u8); // tiny int: the byte is the value
    } else if (i8::MIN as i64..=i8::MAX as i64).contains(&i) {
        out.push(0xC8);
        out.push((i as i8) as u8);
    } else if (i16::MIN as i64..=i16::MAX as i64).contains(&i) {
        out.push(0xC9);
        out.extend_from_slice(&(i as i16).to_be_bytes());
    } else if (i32::MIN as i64..=i32::MAX as i64).contains(&i) {
        out.push(0xCA);
        out.extend_from_slice(&(i as i32).to_be_bytes());
    } else {
        out.push(0xCB);
        out.extend_from_slice(&i.to_be_bytes());
    }
}

// ---- decoding ----------------------------------------------------------

/// A cursor-based PackStream decoder over a borrowed buffer.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Reader<'a> {
    pub fn position(&self) -> usize {
        self.pos
    }
    pub fn new(buf: &'a [u8]) -> Self {
        Reader {
            buf,
            pos: 0,
            depth: 0,
        }
    }

    fn byte(&mut self) -> Result<u8, String> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or("unexpected end of packstream")?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or("packstream length overflow")?;
        let s = self
            .buf
            .get(self.pos..end)
            .ok_or("unexpected end of packstream")?;
        self.pos = end;
        Ok(s)
    }

    fn be_u16(&mut self) -> Result<usize, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize)
    }
    fn be_u32(&mut self) -> Result<usize, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }

    /// Decode the next value.
    pub fn read(&mut self) -> Result<Ps, String> {
        // A byte limit alone cannot prevent deeply nested input from exhausting
        // the thread stack. Limit recursive decoding independently.
        if self.depth >= 64 {
            return Err("packstream nesting exceeds 64 levels".into());
        }
        self.depth += 1;
        let result = self.read_value();
        self.depth -= 1;
        result
    }

    fn read_value(&mut self) -> Result<Ps, String> {
        let m = self.byte()?;
        Ok(match m {
            0xC0 => Ps::Null,
            0xC2 => Ps::Bool(false),
            0xC3 => Ps::Bool(true),
            0xC1 => Ps::Float(f64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            0xC8 => Ps::Int(self.byte()? as i8 as i64),
            0xC9 => Ps::Int(i16::from_be_bytes(self.take(2)?.try_into().unwrap()) as i64),
            0xCA => Ps::Int(i32::from_be_bytes(self.take(4)?.try_into().unwrap()) as i64),
            0xCB => Ps::Int(i64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            // tiny int: 0x00..0x7F positive, 0xF0..0xFF negative
            b if b <= 0x7F => Ps::Int(b as i64),
            b if b >= 0xF0 => Ps::Int(b as i8 as i64),
            // strings
            b if (0x80..=0x8F).contains(&b) => self.read_string((b & 0x0F) as usize)?,
            0xD0 => {
                let n = self.byte()? as usize;
                self.read_string(n)?
            }
            0xD1 => {
                let n = self.be_u16()?;
                self.read_string(n)?
            }
            0xD2 => {
                let n = self.be_u32()?;
                self.read_string(n)?
            }
            // bytes
            0xCC => {
                let n = self.byte()? as usize;
                Ps::Bytes(self.take(n)?.to_vec())
            }
            0xCD => {
                let n = self.be_u16()?;
                Ps::Bytes(self.take(n)?.to_vec())
            }
            0xCE => {
                let n = self.be_u32()?;
                Ps::Bytes(self.take(n)?.to_vec())
            }
            // lists
            b if (0x90..=0x9F).contains(&b) => self.read_list((b & 0x0F) as usize)?,
            0xD4 => {
                let n = self.byte()? as usize;
                self.read_list(n)?
            }
            0xD5 => {
                let n = self.be_u16()?;
                self.read_list(n)?
            }
            0xD6 => {
                let n = self.be_u32()?;
                self.read_list(n)?
            }
            // maps
            b if (0xA0..=0xAF).contains(&b) => self.read_map((b & 0x0F) as usize)?,
            0xD8 => {
                let n = self.byte()? as usize;
                self.read_map(n)?
            }
            0xD9 => {
                let n = self.be_u16()?;
                self.read_map(n)?
            }
            0xDA => {
                let n = self.be_u32()?;
                self.read_map(n)?
            }
            // structures
            b if (0xB0..=0xBF).contains(&b) => {
                let nfields = (b & 0x0F) as usize;
                let sig = self.byte()?;
                let mut fields = Vec::with_capacity(nfields);
                for _ in 0..nfields {
                    fields.push(self.read()?);
                }
                Ps::Struct(sig, fields)
            }
            other => return Err(format!("unknown packstream marker 0x{other:02X}")),
        })
    }

    fn read_string(&mut self, n: usize) -> Result<Ps, String> {
        let bytes = self.take(n)?;
        String::from_utf8(bytes.to_vec())
            .map(Ps::String)
            .map_err(|_| "invalid utf-8 string".to_string())
    }
    fn read_list(&mut self, n: usize) -> Result<Ps, String> {
        // Every item needs at least one encoded byte. Validate the declared
        // count against actual input before attempting any allocation.
        if n > self.buf.len() - self.pos {
            return Err("list count exceeds remaining packstream bytes".into());
        }
        let mut items = Vec::new();
        items
            .try_reserve_exact(n)
            .map_err(|_| "cannot allocate packstream list")?;
        for _ in 0..n {
            items.push(self.read()?);
        }
        Ok(Ps::List(items))
    }
    fn read_map(&mut self, n: usize) -> Result<Ps, String> {
        if n > (self.buf.len() - self.pos) / 2 {
            return Err("map count exceeds remaining packstream bytes".into());
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(n)
            .map_err(|_| "cannot allocate packstream map")?;
        for _ in 0..n {
            let key = match self.read()? {
                Ps::String(s) => s,
                other => return Err(format!("map key must be a string, got {other:?}")),
            };
            entries.push((key, self.read()?));
        }
        Ok(Ps::Map(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(v: Ps) {
        let mut buf = Vec::new();
        encode(&v, &mut buf);
        let got = Reader::new(&buf).read().expect("decode");
        assert_eq!(got, v, "value should survive encodeâ†’decode");
    }

    #[test]
    fn primitives_round_trip() {
        round_trip(Ps::Null);
        round_trip(Ps::Bool(true));
        round_trip(Ps::Bool(false));
        round_trip(Ps::Float(3.5));
        for i in [
            -16,
            -1,
            0,
            42,
            127,
            128,
            -200,
            70_000,
            -3_000_000_000,
            i64::MIN,
            i64::MAX,
        ] {
            round_trip(Ps::Int(i));
        }
    }

    #[test]
    fn strings_pick_the_right_size_class() {
        round_trip(Ps::String(String::new())); // tiny empty
        round_trip(Ps::String("hi".into())); // tiny
        round_trip(Ps::String("x".repeat(200))); // str8
        round_trip(Ps::String("y".repeat(70_000))); // str32
    }

    #[test]
    fn containers_and_structs_round_trip() {
        round_trip(Ps::List(vec![Ps::Int(1), Ps::String("a".into()), Ps::Null]));
        round_trip(Ps::Map(vec![
            ("k".into(), Ps::Int(9)),
            ("s".into(), Ps::String("v".into())),
        ]));
        // A RUN-message-shaped struct (signature 0x10, three fields).
        round_trip(Ps::Struct(
            0x10,
            vec![
                Ps::String("RETURN 1".into()),
                Ps::Map(vec![]),
                Ps::Map(vec![]),
            ],
        ));
    }
}
