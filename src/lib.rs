//! Native Bolt driver for Galactus DB: Bolt 5.0 to 5.4 with a Bolt 4.4
//! fallback, one connection per driver, no write retries.
//!
//! The default build uses only the standard library. Verified `bolt+s://`
//! needs the `tls` cargo feature (rustls with the platform verifier).
#[cfg(feature = "tls")]
mod tls;
mod value;
mod wire;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    time::Duration as Timeout,
};
pub use value::*;
pub type Map = BTreeMap<String, Value>;
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;
/// The driver release, also sent in `user_agent` and `bolt_agent`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const USER_AGENT: &str = concat!("galactus-db-rust-driver/", env!("CARGO_PKG_VERSION"));

/// Server error codes published by Galactus, for comparison with
/// [`Error::code`]. Codes the server adds later still arrive as plain strings.
pub mod codes {
    pub const CONSTRAINT_VALIDATION_FAILED: &str =
        "Galactus.ClientError.Schema.ConstraintValidationFailed";
    pub const TYPE_ERROR: &str = "Galactus.ClientError.Statement.TypeError";
    pub const ARGUMENT_ERROR: &str = "Galactus.ClientError.Statement.ArgumentError";
    pub const SQL_TOO_MANY_ROWS: &str = "Galactus.ClientError.Statement.SqlTooManyRows";
    pub const SQL_CAST_FAILED: &str = "Galactus.ClientError.Statement.SqlCastFailed";
    pub const TABLE_TOO_LARGE: &str = "Galactus.ClientError.Statement.TableTooLarge";
    pub const UNSUPPORTED_FEATURE: &str = "Galactus.DatabaseError.Storage.UnsupportedFeature";
    pub const PROCEDURE_NOT_FOUND: &str = "Galactus.ClientError.Procedure.ProcedureNotFound";
    pub const LICENSE_RESTRICTED: &str = "Galactus.ClientError.License.Restricted";
    pub const READ_ONLY_REPLICA: &str = "Galactus.ClientError.Database.ReadOnlyReplica";
    pub const TRANSACTION_TIMED_OUT: &str = "Galactus.ClientError.Transaction.TimedOut";
    pub const TRANSACTION_TERMINATED: &str = "Galactus.ClientError.Transaction.Terminated";
    pub const SNAPSHOT_TOO_OLD: &str = "Galactus.TransientError.Transaction.SnapshotTooOld";
    pub const FORBIDDEN: &str = "Galactus.ClientError.Security.Forbidden";
    pub const SYNTAX_ERROR: &str = "Neo.ClientError.Statement.SyntaxError";
    pub const SEMANTIC_ERROR: &str = "Neo.ClientError.Statement.SemanticError";
    pub const UNAUTHORIZED: &str = "Neo.ClientError.Security.Unauthorized";
    pub const DATABASE_NOT_FOUND: &str = "Neo.ClientError.Database.DatabaseNotFound";
}

#[derive(Debug)]
pub enum Error {
    /// Reading or writing the socket failed; the connection is discarded.
    Io(io::Error),
    /// The peer broke the protocol or the request was invalid.
    Protocol(String),
    /// The server answered with FAILURE. After a query, BEGIN, COMMIT or
    /// ROLLBACK the driver has already reset the connection and stays usable;
    /// any open transaction is closed.
    Database {
        code: String,
        message: String,
        metadata: Map,
    },
    /// The URI or options are not usable (unsupported scheme, a trusted
    /// certificate file on `bolt://`, `bolt+s://` without the `tls` feature).
    Config(String),
    /// The TLS handshake failed, for example because the server certificate
    /// could not be verified.
    Tls(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Protocol(e) | Self::Config(e) | Self::Tls(e) => write!(f, "{e}"),
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
impl Error {
    /// The server code of a database error.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Database { code, .. } => Some(code),
            _ => None,
        }
    }
    /// `[vendor, classification, category, title]` when the code has four parts.
    fn code_parts(&self) -> Option<[&str; 4]> {
        let mut parts = self.code()?.split('.');
        let p = [parts.next()?, parts.next()?, parts.next()?, parts.next()?];
        if parts.next().is_some() || p.iter().any(|s| s.is_empty()) {
            return None;
        }
        Some(p)
    }
    /// `ClientError`, `TransientError` or `DatabaseError`.
    pub fn classification(&self) -> Option<&str> {
        self.code_parts().map(|p| p[1])
    }
    pub fn category(&self) -> Option<&str> {
        self.code_parts().map(|p| p[2])
    }
    pub fn title(&self) -> Option<&str> {
        self.code_parts().map(|p| p[3])
    }
    pub fn is_client_error(&self) -> bool {
        self.classification() == Some("ClientError")
    }
    pub fn is_transient(&self) -> bool {
        self.classification() == Some("TransientError")
    }
    /// The server classified the failure as `DatabaseError`.
    pub fn is_database_error(&self) -> bool {
        self.classification() == Some("DatabaseError")
    }
    /// True for `TransientError` codes only. The driver never retries by itself.
    pub fn is_retryable(&self) -> bool {
        self.is_transient()
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// The value forms a codec writes: Bolt 4.4 or Bolt 5.x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Datetimes as `F`, graph values without element ids.
    Bolt44,
    /// Datetimes as `I`, graph values with element ids.
    Bolt5,
}

/// Encode in the Bolt 4.4 forms (the 0.1 behaviour).
pub fn encode(value: &Value) -> Result<Vec<u8>> {
    encode_for(value, Protocol::Bolt44)
}
/// Encode in the forms of the given protocol.
pub fn encode_for(value: &Value, protocol: Protocol) -> Result<Vec<u8>> {
    let raw = value.to_wire(0, protocol)?;
    let mut bytes = Vec::new();
    wire::encode(&raw, &mut bytes);
    if bytes.len() > MAX_MESSAGE {
        return Err(Error::Protocol("message exceeds 64 MiB".into()));
    }
    Ok(bytes)
}
/// Decode one value. Bolt 4.4 and Bolt 5 forms are both accepted.
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

/// Options for [`Driver::connect_with_options`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ConnectOptions {
    /// A PEM file with one or more certificates that replaces the platform
    /// trust store for `bolt+s://`. Setting it on `bolt://` is an error.
    pub trusted_certificates: Option<PathBuf>,
}
impl ConnectOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn trusted_certificates(mut self, path: impl Into<PathBuf>) -> Self {
        self.trusted_certificates = Some(path.into());
        self
    }
}

enum Stream {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(Box<tls::TlsStream>),
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            #[cfg(feature = "tls")]
            Self::Tls(s) => s.read(buf),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            #[cfg(feature = "tls")]
            Self::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            #[cfg(feature = "tls")]
            Self::Tls(s) => s.flush(),
        }
    }
}
impl Stream {
    fn shutdown(self) {
        match self {
            Self::Plain(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            #[cfg(feature = "tls")]
            Self::Tls(mut s) => {
                s.conn.send_close_notify();
                let _ = s.flush();
                let _ = s.sock.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

/// A parsed `bolt://` or `bolt+s://` URI.
struct Target {
    tls: bool,
    host: String,
    addr: String,
}
fn config(s: impl Into<String>) -> Error {
    Error::Config(s.into())
}
fn parse_uri(uri: &str) -> Result<Target> {
    let (scheme, rest) = uri
        .split_once("://")
        .ok_or_else(|| config("expected bolt://host:port or bolt+s://host:port"))?;
    let tls = match scheme {
        "bolt" => false,
        "bolt+s" => true,
        "bolt+ssc" => {
            return Err(config(
                "bolt+ssc:// (unverified TLS) is not supported; use bolt+s:// and, for a \
                 private CA or self-signed server, the trusted_certificates option",
            ))
        }
        "neo4j" | "neo4j+s" | "neo4j+ssc" => {
            return Err(config(
                "routing URIs are not supported; use bolt:// or bolt+s://",
            ))
        }
        other => return Err(config(format!("unsupported URI scheme {other}://"))),
    };
    let addr = rest.strip_suffix('/').unwrap_or(rest);
    if addr.is_empty() || addr.chars().any(|c| matches!(c, '/' | '?' | '#' | '@')) {
        return Err(config(
            "invalid Bolt URI: give host[:port] only; credentials and options are separate arguments",
        ));
    }
    let (host, addr) = if let Some(inner) = addr.strip_prefix('[') {
        let (host, port) = inner
            .split_once(']')
            .ok_or_else(|| config("invalid Bolt URI"))?;
        let addr = if port.is_empty() {
            format!("{addr}:7687")
        } else if port.starts_with(':') {
            addr.into()
        } else {
            return Err(config("invalid Bolt URI"));
        };
        (host.to_string(), addr)
    } else if let Some((host, _)) = addr.split_once(':') {
        (host.to_string(), addr.into())
    } else {
        (addr.to_string(), format!("{addr}:7687"))
    };
    Ok(Target { tls, host, addr })
}

fn bolt_agent() -> Value {
    let language = match option_env!("GALACTUS_RUSTC_VERSION") {
        Some(v) if !v.is_empty() => format!("Rust/{v}"),
        _ => "Rust".to_string(),
    };
    let mut agent = Map::from([
        ("product".into(), USER_AGENT.into()),
        ("language".into(), language.into()),
        (
            "platform".into(),
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH).into(),
        ),
    ]);
    if let Some(details) = option_env!("GALACTUS_RUSTC_DETAILS") {
        agent.insert("language_details".into(), details.into());
    }
    Value::Map(agent)
}

pub struct Driver {
    stream: Option<Stream>,
    database: String,
    transaction: bool,
    tx_timeout: Option<Timeout>,
    version: (u8, u8),
    protocol_version: String,
    server_agent: String,
    connection_id: String,
    last_bookmark: Option<String>,
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
const SUCCESS: u8 = 0x70;
const RECORD: u8 = 0x71;
const IGNORED: u8 = 0x7e;
const FAILURE: u8 = 0x7f;
impl Driver {
    /// Connect to `bolt://host[:port]`, or verified `bolt+s://host[:port]`
    /// when the `tls` feature is enabled, trusting the platform store.
    /// An empty `database` lets the server choose its configured default.
    pub fn connect(
        uri: &str,
        username: &str,
        password: &str,
        database: &str,
        timeout: Timeout,
    ) -> Result<Self> {
        Self::connect_with_options(
            uri,
            username,
            password,
            database,
            timeout,
            &ConnectOptions::default(),
        )
    }
    /// [`connect`](Self::connect) with options such as a trusted certificate file.
    pub fn connect_with_options(
        uri: &str,
        username: &str,
        password: &str,
        database: &str,
        timeout: Timeout,
        options: &ConnectOptions,
    ) -> Result<Self> {
        let target = parse_uri(uri)?;
        if options.trusted_certificates.is_some() && !target.tls {
            return Err(config(
                "trusted_certificates applies to bolt+s:// only; bolt:// is plaintext",
            ));
        }
        if cfg!(not(feature = "tls")) && target.tls {
            return Err(config(
                "bolt+s:// needs the `tls` cargo feature of galactus-db-rust-driver; \
                 this build has no TLS support",
            ));
        }
        #[cfg(feature = "tls")]
        let tls_config = if target.tls {
            Some(tls::client_config(options.trusted_certificates.as_deref())?)
        } else {
            None
        };
        let mut connected = None;
        let mut last = None;
        for a in target.addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&a, timeout) {
                Ok(s) => {
                    connected = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let s = connected.ok_or_else(|| {
            Error::Io(last.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no address")))
        })?;
        s.set_read_timeout(Some(timeout))?;
        s.set_write_timeout(Some(timeout))?;
        s.set_nodelay(true)?;
        #[cfg(feature = "tls")]
        let mut s = match tls_config {
            Some(c) => Stream::Tls(Box::new(tls::connect(c, &target.host, s)?)),
            None => Stream::Plain(s),
        };
        #[cfg(not(feature = "tls"))]
        let mut s = Stream::Plain(s);
        let _ = &target.host;
        // Proposals: 5.4 down to 5.0, then 4.4, then two empty slots.
        s.write_all(&[
            0x60, 0x60, 0xb0, 0x17, 0, 4, 4, 5, 0, 0, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0,
        ])?;
        s.flush()?;
        let mut chosen = [0; 4];
        s.read_exact(&mut chosen)?;
        let version = match chosen {
            [0, 0, minor @ 0..=4, 5] => (5, minor),
            [0, 0, 4, 4] => (4, 4),
            _ => {
                s.shutdown();
                return Err(protocol(
                    "server did not agree a supported Bolt version (5.0-5.4 or 4.4)",
                ));
            }
        };
        let mut d = Self {
            stream: Some(s),
            database: database.into(),
            transaction: false,
            tx_timeout: None,
            version,
            protocol_version: format!("{}.{}", version.0, version.1),
            server_agent: String::new(),
            connection_id: String::new(),
            last_bookmark: None,
        };
        let auth = [
            ("scheme".to_string(), Value::from("basic")),
            ("principal".to_string(), username.into()),
            ("credentials".to_string(), password.into()),
        ];
        let mut hello = Map::from([("user_agent".into(), USER_AGENT.into())]);
        let logon = version >= (5, 1);
        if logon {
            hello.insert("bolt_agent".into(), bolt_agent());
        } else {
            hello.extend(auth.clone());
        }
        // Any failure here drops `d`, which closes the connection.
        d.send(0x01, vec![Value::Map(hello)])?;
        let meta = d.success()?;
        let text = |key: &str| match meta.get(key) {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        d.server_agent = text("server");
        d.connection_id = text("connection_id");
        if logon {
            // LOGON only after HELLO's SUCCESS; never pipelined.
            d.send(0x6a, vec![Value::Map(Map::from(auth))])?;
            d.success()?;
        }
        Ok(d)
    }
    /// The agreed Bolt version, such as `"5.4"` or `"4.4"`.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }
    /// The value forms used on this connection.
    pub fn protocol(&self) -> Protocol {
        if self.version.0 >= 5 {
            Protocol::Bolt5
        } else {
            Protocol::Bolt44
        }
    }
    /// The `server` agent string from HELLO's SUCCESS.
    pub fn server_agent(&self) -> &str {
        &self.server_agent
    }
    /// The `connection_id` from HELLO's SUCCESS.
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
    /// The most recent bookmark from an autocommit query or a COMMIT.
    pub fn last_bookmark(&self) -> Option<&str> {
        self.last_bookmark.as_deref()
    }
    /// Ask the server to stop each later autocommit query, or transaction from
    /// its `BEGIN`, after `timeout` (Bolt `tx_timeout`). `None` leaves only the
    /// server's own limit. The socket timeout given to `connect` still applies.
    pub fn set_transaction_timeout(&mut self, timeout: Option<Timeout>) {
        self.tx_timeout = timeout;
    }
    fn timeout_entry(&self) -> Option<(String, Value)> {
        self.tx_timeout.map(|t| {
            let ms = i64::try_from(t.as_millis()).unwrap_or(i64::MAX).max(1);
            ("tx_timeout".to_string(), Value::Int(ms))
        })
    }
    fn stream(&mut self) -> Result<&mut Stream> {
        self.stream
            .as_mut()
            .ok_or_else(|| protocol("driver is closed"))
    }
    fn send(&mut self, tag: u8, mut fields: Vec<Value>) -> Result<()> {
        if tag == 0x10 || tag == 0x11 {
            let index = if tag == 0x10 { 2 } else { 0 };
            if let Some(Value::Map(extra)) = fields.get_mut(index) {
                if extra.get("db") == Some(&Value::String(String::new())) {
                    extra.remove("db");
                }
            }
        }
        let bytes = encode_for(&Value::Structure(tag, fields), self.protocol())?;
        let mut frame = Vec::new();
        for chunk in bytes.chunks(65535) {
            frame.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            frame.extend_from_slice(chunk)
        }
        frame.extend_from_slice(&[0, 0]);
        let stream = self.stream()?;
        stream.write_all(&frame)?;
        stream.flush()?;
        Ok(())
    }
    /// The next response: SUCCESS, RECORD, IGNORED or FAILURE with its field.
    fn receive_raw(&mut self) -> Result<(u8, Value)> {
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
            if matches!(tag, SUCCESS | RECORD | FAILURE) && fields.len() == 1 {
                return Ok((tag, fields.remove(0)));
            }
            if tag == IGNORED && fields.len() <= 1 {
                return Ok((tag, Value::Null));
            }
        }
        Err(protocol("unexpected Bolt response"))
    }
    /// SUCCESS or RECORD; FAILURE becomes [`Error::Database`].
    fn receive(&mut self) -> Result<(u8, Value)> {
        match self.receive_raw()? {
            (FAILURE, value) => {
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
                Err(Error::Database {
                    code,
                    message,
                    metadata,
                })
            }
            (IGNORED, _) => Err(protocol("unexpected IGNORED")),
            other => Ok(other),
        }
    }
    fn success(&mut self) -> Result<Map> {
        let (tag, value) = self.receive()?;
        if tag != SUCCESS {
            return Err(protocol("expected SUCCESS"));
        }
        map(value)
    }
    /// Run one request exchange. After a database FAILURE send RESET so the
    /// connection stays usable (the server rolls back any open transaction);
    /// after any other failure, discard the connection.
    fn guarded<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let result = f(self);
        match &result {
            Ok(_) => {}
            Err(Error::Database { .. }) => {
                self.transaction = false;
                if self.reset().is_err() {
                    self.close()
                }
            }
            Err(_) => self.close(),
        }
        result
    }
    fn reset(&mut self) -> Result<()> {
        self.send(0x0f, vec![])?;
        loop {
            match self.receive_raw()?.0 {
                SUCCESS => return Ok(()),
                IGNORED => continue,
                _ => return Err(protocol("RESET did not succeed")),
            }
        }
    }
    fn validate(params: &Map) -> Result<()> {
        for v in params.values() {
            // Graph values and unknown structures are result-only in every form.
            validate_parameter_wire(&v.to_wire(0, Protocol::Bolt44)?)?;
        }
        Ok(())
    }
    pub fn execute_query(&mut self, query: &str, params: Map) -> Result<QueryResult> {
        Self::validate(&params)?;
        self.guarded(|d| d.query_inner(query, params, None))
            .map(|(result, _)| result)
    }
    /// Like [`execute_query`](Self::execute_query), but pulls at most
    /// `max_records` records and discards the rest on the server. The flag is
    /// `true` when records were discarded.
    pub fn execute_query_limited(
        &mut self,
        query: &str,
        params: Map,
        max_records: usize,
    ) -> Result<(QueryResult, bool)> {
        Self::validate(&params)?;
        self.guarded(|d| d.query_inner(query, params, Some(max_records)))
    }
    fn query_inner(
        &mut self,
        query: &str,
        params: Map,
        limit: Option<usize>,
    ) -> Result<(QueryResult, bool)> {
        let extra = if self.transaction {
            Map::new()
        } else {
            let mut extra = Map::from([("db".into(), self.database.clone().into())]);
            extra.extend(self.timeout_entry());
            extra
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
        let n = limit.map_or(-1, |l| i64::try_from(l).unwrap_or(i64::MAX));
        self.send(0x3f, vec![Value::Map(Map::from([("n".into(), n.into())]))])?;
        let mut truncated = false;
        loop {
            let (tag, value) = self.receive()?;
            if tag == SUCCESS {
                let summary = map(value)?;
                if summary.get("has_more") == Some(&Value::Bool(true)) {
                    // Over the limit: DISCARD the rest so the summary still arrives.
                    let next = if limit.is_some() {
                        truncated = true;
                        0x2f
                    } else {
                        0x3f
                    };
                    self.send(
                        next,
                        vec![Value::Map(Map::from([("n".into(), (-1i64).into())]))],
                    )?;
                    continue;
                }
                if let Some(Value::String(b)) = summary.get("bookmark") {
                    self.last_bookmark = Some(b.clone());
                }
                meta.extend(summary);
                return Ok((
                    QueryResult {
                        keys,
                        records,
                        summary: meta,
                    },
                    truncated,
                ));
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
        self.guarded(|d| {
            let mut extra = Map::from([
                ("db".into(), d.database.clone().into()),
                ("mode".into(), if read_only { "r" } else { "w" }.into()),
            ]);
            extra.extend(d.timeout_entry());
            d.send(0x11, vec![Value::Map(extra)])?;
            d.success()?;
            d.transaction = true;
            Ok(())
        })
    }
    fn finish(&mut self, tag: u8) -> Result<Map> {
        if !self.transaction {
            return Err(protocol("no transaction"));
        }
        self.guarded(|d| {
            d.send(tag, vec![])?;
            let m = d.success()?;
            d.transaction = false;
            if let Some(Value::String(b)) = m.get("bookmark") {
                d.last_bookmark = Some(b.clone());
            }
            Ok(m)
        })
    }
    /// Commit the open transaction. A database error means it did not commit.
    /// An I/O error means the outcome is unknown: never retry it automatically.
    pub fn commit(&mut self) -> Result<Map> {
        self.finish(0x12)
    }
    pub fn rollback(&mut self) -> Result<()> {
        self.finish(0x13).map(|_| ())
    }
    pub fn close(&mut self) {
        self.transaction = false;
        if let Some(s) = self.stream.take() {
            s.shutdown();
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
            if ![
                0x44, 0x74, 0x54, 0x64, 0x46, 0x66, 0x49, 0x69, 0x45, 0x58, 0x59,
            ]
            .contains(tag)
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_parsing() {
        let t = parse_uri("bolt://localhost:7688").unwrap();
        assert!(!t.tls);
        assert_eq!(
            (t.host.as_str(), t.addr.as_str()),
            ("localhost", "localhost:7688")
        );
        let t = parse_uri("bolt+s://db.example.com/").unwrap();
        assert!(t.tls);
        assert_eq!(t.addr, "db.example.com:7687");
        let t = parse_uri("bolt://[::1]").unwrap();
        assert_eq!((t.host.as_str(), t.addr.as_str()), ("::1", "[::1]:7687"));
        for bad in [
            "bolt+ssc://h",
            "neo4j://h",
            "neo4j+s://h",
            "neo4j+ssc://h",
            "http://h",
            "bolt://u:p@h",
            "bolt://h?tls=1",
            "h:7687",
        ] {
            assert!(matches!(parse_uri(bad), Err(Error::Config(_))), "{bad}");
        }
    }

    #[test]
    fn trusted_certificates_on_plaintext_is_a_configuration_error() {
        let options = ConnectOptions::new().trusted_certificates("ca.pem");
        let r = Driver::connect_with_options(
            "bolt://127.0.0.1:1",
            "u",
            "p",
            "",
            Timeout::from_secs(1),
            &options,
        );
        assert!(matches!(r, Err(Error::Config(_))));
    }

    #[cfg(not(feature = "tls"))]
    #[test]
    fn default_build_rejects_bolt_s_naming_the_feature() {
        match Driver::connect("bolt+s://127.0.0.1:1", "u", "p", "", Timeout::from_secs(1)) {
            Err(Error::Config(m)) => assert!(m.contains("`tls` cargo feature"), "{m}"),
            Err(e) => panic!("unexpected error {e}"),
            Ok(_) => panic!("bolt+s:// connected without the tls feature"),
        }
    }

    #[test]
    fn error_classification() {
        let e = Error::Database {
            code: codes::SNAPSHOT_TOO_OLD.into(),
            message: "m".into(),
            metadata: Map::new(),
        };
        assert_eq!(e.classification(), Some("TransientError"));
        assert_eq!(e.category(), Some("Transaction"));
        assert_eq!(e.title(), Some("SnapshotTooOld"));
        assert!(e.is_transient() && e.is_retryable());
        assert!(!e.is_client_error() && !e.is_database_error());
        let e = Error::Database {
            code: codes::UNSUPPORTED_FEATURE.into(),
            message: "m".into(),
            metadata: Map::new(),
        };
        assert!(e.is_database_error() && !e.is_retryable());
        let e = Error::Database {
            code: "Odd".into(),
            message: "m".into(),
            metadata: Map::new(),
        };
        assert_eq!(
            (e.classification(), e.category(), e.title()),
            (None, None, None)
        );
        assert!(!e.is_client_error() && !e.is_retryable());
        assert_eq!(protocol("x").code(), None);
    }

    #[test]
    fn bolt_agent_names_the_driver() {
        let Value::Map(agent) = bolt_agent() else {
            panic!("bolt_agent is a map")
        };
        assert_eq!(
            agent["product"],
            Value::from(concat!(
                "galactus-db-rust-driver/",
                env!("CARGO_PKG_VERSION")
            ))
        );
        assert!(agent["language"].as_str().unwrap().starts_with("Rust"));
    }
}
