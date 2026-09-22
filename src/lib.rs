//! Native, dependency-free Bolt 4.4. One connection per worker; no write retries.
mod value;
mod wire;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration as Timeout,
};
pub use value::*;
pub type Map = BTreeMap<String, Value>;
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Protocol(String),
    Database {
        code: String,
        message: String,
        metadata: Map,
    },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Protocol(e) => write!(f, "{e}"),
            Self::Database { code, message, .. } => write!(f, "{code}: {message}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<String> for Error {
    fn from(e: String) -> Self {
        Self::Protocol(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub fn encode(value: &Value) -> Result<Vec<u8>> {
    let raw = value.to_wire(0)?;
    let mut bytes = Vec::new();
    wire::encode(&raw, &mut bytes);
    if bytes.len() > MAX_MESSAGE {
        return Err(Error::Protocol("message exceeds 64 MiB".into()));
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Value> {
    let mut reader = wire::Reader::new(bytes);
    let raw = reader.read()?;
    if reader.position() != bytes.len() {
        return Err(Error::Protocol("trailing PackStream bytes".into()));
    }
    Value::from_wire(raw)
}
#[derive(Debug)]
pub struct QueryResult {
    pub keys: Vec<String>,
    pub records: Vec<Map>,
    pub summary: Map,
}
pub struct Driver {
    stream: Option<TcpStream>,
    database: String,
    transaction: bool,
}
fn protocol(s: &str) -> Error {
    Error::Protocol(s.into())
}
fn map(value: Value) -> Result<Map> {
    if let Value::Map(m) = value {
        Ok(m)
    } else {
        Err(protocol("expected map"))
    }
}
impl Driver {
    /// Direct TCP. TLS requires an external tunnel in this standard-library-only build.
    pub fn connect(
        uri: &str,
        username: &str,
        password: &str,
        database: &str,
        timeout: Timeout,
    ) -> Result<Self> {
        let addr = uri
            .strip_prefix("bolt://")
            .ok_or_else(|| protocol("expected bolt://host:port; TLS is not built in"))?;
        let addr = addr.strip_suffix('/').unwrap_or(addr);
        if addr.is_empty() || addr.chars().any(|c| matches!(c, '/' | '?' | '#' | '@')) {
            return Err(protocol("invalid Bolt URI"));
        }
        let addr = if addr.starts_with('[') {
            if addr.ends_with(']') {
                format!("{addr}:7687")
            } else {
                addr.into()
            }
        } else if !addr.contains(':') {
            format!("{addr}:7687")
        } else {
            addr.into()
        };
        let mut connected = None;
        let mut last = None;
        for a in addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&a, timeout) {
                Ok(s) => {
                    connected = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let mut s = connected.ok_or_else(|| {
            Error::Io(last.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no address")))
        })?;
        s.set_read_timeout(Some(timeout))?;
        s.set_write_timeout(Some(timeout))?;
        s.set_nodelay(true)?;
        s.write_all(&[
            0x60, 0x60, 0xb0, 0x17, 0, 0, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])?;
        let mut chosen = [0; 4];
        s.read_exact(&mut chosen)?;
        if chosen != [0, 0, 4, 4] {
            return Err(protocol("server did not select Bolt 4.4"));
        }
        let mut d = Self {
            stream: Some(s),
            database: database.into(),
            transaction: false,
        };
        d.send(
            1,
            vec![Value::Map(BTreeMap::from([
                ("user_agent".into(), "galactus-rust/0.1".into()),
                ("scheme".into(), "basic".into()),
                ("principal".into(), username.into()),
                ("credentials".into(), password.into()),
            ]))],
        )?;
        d.success()?;
        Ok(d)
    }
    fn stream(&mut self) -> Result<&mut TcpStream> {
        self.stream
            .as_mut()
            .ok_or_else(|| protocol("driver is closed"))
    }
    fn send(&mut self, tag: u8, fields: Vec<Value>) -> Result<()> {
        let bytes = encode(&Value::Structure(tag, fields))?;
        let mut frame = Vec::new();
        for chunk in bytes.chunks(65535) {
            frame.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            frame.extend_from_slice(chunk)
        }
        frame.extend_from_slice(&[0, 0]);
        self.stream()?.write_all(&frame)?;
        Ok(())
    }
    fn receive(&mut self) -> Result<(u8, Value)> {
        let mut bytes = Vec::new();
        loop {
            let mut h = [0; 2];
            self.stream()?.read_exact(&mut h)?;
            let n = u16::from_be_bytes(h) as usize;
            if n == 0 {
                if !bytes.is_empty() {
                    break;
                }
                continue;
            }
            if bytes.len() + n > MAX_MESSAGE {
                return Err(protocol("message exceeds 64 MiB"));
            }
            let p = bytes.len();
            bytes.resize(p + n, 0);
            self.stream()?.read_exact(&mut bytes[p..])?;
        }
        if let Value::Structure(tag, mut fields) = decode(&bytes)? {
            if fields.len() != 1 {
                return Err(protocol("invalid Bolt response"));
            }
            let value = fields.remove(0);
            if tag == 0x7f {
                let metadata = map(value)?;
                let code = metadata
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("DatabaseError")
                    .into();
                let message = metadata
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Database failure")
                    .into();
                return Err(Error::Database {
                    code,
                    message,
                    metadata,
                });
            }
            if tag == 0x70 || tag == 0x71 {
                return Ok((tag, value));
            }
        }
        Err(protocol("unexpected Bolt response"))
    }
    fn success(&mut self) -> Result<Map> {
        let (tag, value) = self.receive()?;
        if tag != 0x70 {
            return Err(protocol("expected SUCCESS"));
        }
        map(value)
    }
    pub fn execute_query(&mut self, query: &str, params: Map) -> Result<QueryResult> {
        let result = self.query_inner(query, params);
        if result.is_err() {
            self.close()
        }
        result
    }
    fn query_inner(&mut self, query: &str, params: Map) -> Result<QueryResult> {
        for v in params.values() {
            validate_parameter_wire(&v.to_wire(0)?)?;
        }
        let extra = if self.transaction {
            Map::new()
        } else {
            Map::from([("db".into(), self.database.clone().into())])
        };
        self.send(
            0x10,
            vec![query.into(), Value::Map(params), Value::Map(extra)],
        )?;
        let mut meta = self.success()?;
        let keys = if let Some(Value::List(keys)) = meta.get("fields") {
            keys.iter()
                .map(|k| {
                    k.as_str()
                        .map(String::from)
                        .ok_or_else(|| protocol("invalid field name"))
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            return Err(protocol("missing fields"));
        };
        let mut records = Vec::new();
        self.send(
            0x3f,
            vec![Value::Map(Map::from([("n".into(), (-1i64).into())]))],
        )?;
        loop {
            let (tag, value) = self.receive()?;
            if tag == 0x70 {
                let summary = map(value)?;
                if summary.get("has_more") == Some(&Value::Bool(true)) {
                    self.send(
                        0x3f,
                        vec![Value::Map(Map::from([("n".into(), (-1i64).into())]))],
                    )?;
                    continue;
                }
                meta.extend(summary);
                return Ok(QueryResult {
                    keys,
                    records,
                    summary: meta,
                });
            }
            if let Value::List(values) = value {
                if values.len() != keys.len() {
                    return Err(protocol("record width mismatch"));
                }
                records.push(keys.iter().cloned().zip(values).collect())
            } else {
                return Err(protocol("invalid RECORD"));
            }
        }
    }
    pub fn begin(&mut self, read_only: bool) -> Result<()> {
        if self.transaction {
            return Err(protocol("transaction already open"));
        }
        let r = (|| {
            self.send(
                0x11,
                vec![Value::Map(Map::from([
                    ("db".into(), self.database.clone().into()),
                    ("mode".into(), if read_only { "r" } else { "w" }.into()),
                ]))],
            )?;
            self.success()?;
            self.transaction = true;
            Ok(())
        })();
        if r.is_err() {
            self.close()
        }
        r
    }
    fn finish(&mut self, tag: u8) -> Result<Map> {
        if !self.transaction {
            return Err(protocol("no transaction"));
        }
        let r = (|| {
            self.send(tag, vec![])?;
            let m = self.success()?;
            self.transaction = false;
            Ok(m)
        })();
        if r.is_err() {
            self.close()
        }
        r
    }
    pub fn commit(&mut self) -> Result<Map> {
        self.finish(0x12)
    }
    pub fn rollback(&mut self) -> Result<()> {
        self.finish(0x13).map(|_| ())
    }
    pub fn close(&mut self) {
        self.transaction = false;
        if let Some(s) = self.stream.take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.close()
    }
}

fn validate_parameter_wire(v: &wire::Ps) -> Result<()> {
    match v {
        wire::Ps::Struct(tag, f) => {
            if ![0x44, 0x74, 0x54, 0x64, 0x46, 0x66, 0x45, 0x58, 0x59].contains(tag) {
                return Err(protocol("graph entities and unknown structures are result-only; pass properties or an ID"));
            }
            for x in f {
                validate_parameter_wire(x)?
            }
        }
        wire::Ps::List(f) => {
            for x in f {
                validate_parameter_wire(x)?
            }
        }
        wire::Ps::Map(m) => {
            for (_, x) in m {
                validate_parameter_wire(x)?
            }
        }
        _ => {}
    };
    Ok(())
}
