# Galactus DB Rust Driver

[Galactus DB](https://galactusdb.com) · [Driver catalogue](https://galactusdb.com/product/drivers) · [Type mapping](docs/BLUEPRINT.md) · [Spatial guide](docs/SPATIAL.md) · [Changelog](CHANGELOG.md) · [Issues](https://github.com/galactusdb/galactus-db-rust-driver/issues)

Connect Rust applications to Galactus DB with parameterised Cypher, native
values and explicit transactions. The driver speaks Bolt 5.4 and falls back to
Bolt 4.4 when a server offers only that. The default build uses the Rust
standard library alone, with **zero third-party dependencies**. Turn on the
`tls` feature for verified `bolt+s://` connections.

**Status: 0.2 / early access.** Source is available on `main`. Each driver owns
one connection and returns complete results. Install from source as shown
below and read the [scope of this release](#scope-of-this-release) for how it
fits your application.

## Contents

- [Quickstart](#quickstart)
- [Connection configuration](#connection-configuration)
- [Encrypted connections](#encrypted-connections)
- [Queries and results](#queries-and-results)
- [Transactions](#transactions)
- [Native type mapping](#native-type-mapping)
- [Spatial values](#spatial-values)
- [Errors and connection lifecycle](#errors-and-connection-lifecycle)
- [Testing](#testing)
- [Scope of this release](#scope-of-this-release)

## Quickstart

### 1. Get the source

Requirements: **Rust 1.70+** for the default build, Git, and a reachable
Galactus DB instance. The `tls` feature follows the minimum Rust version of
`rustls`.

```sh
git clone --branch main https://github.com/galactusdb/galactus-db-rust-driver.git
cd galactus-db-rust-driver
```

Add this to your application’s `Cargo.toml`.

```toml
[dependencies]
galactus = { package = "galactus-db-rust-driver", git = "https://github.com/galactusdb/galactus-db-rust-driver", branch = "main" }
# With verified TLS for bolt+s://:
# galactus = { package = "galactus-db-rust-driver", git = "https://github.com/galactusdb/galactus-db-rust-driver", branch = "main", features = ["tls"] }
```

Git-based installs follow `main`; pin a reviewed commit or tag in your
application for reproducible builds. The driver is installed from source; the
package name does not imply a crates.io release.

### 2. Configure your database credentials

Start Galactus DB on your infrastructure using the setup available from
[galactusdb.com](https://galactusdb.com). The examples use
`bolt://127.0.0.1:7687` and username `gdb`; replace them with your server settings.
Set your configured password in the application environment:

```sh
# Bash / zsh
export GDB_PASSWORD='your-database-password'
```

```powershell
# PowerShell
$env:GDB_PASSWORD = 'your-database-password'
```

`GDB_PASSWORD` is read by these examples; the driver does not load environment
variables or change the server password. Keep credentials out of source control
and connection URIs.

### 3. Run your first query

```rust
use galactus::{Driver, Map};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut driver = Driver::connect("bolt://127.0.0.1:7687", "gdb",
        &std::env::var("GDB_PASSWORD")?, "", Duration::from_secs(30))?;
    let result = driver.execute_query("RETURN $name AS name",
        Map::from([("name".into(), "Ada".into())]))?;
    println!("{}", result.records[0]["name"].as_str().unwrap());
    println!("Bolt {}", driver.protocol_version());
    Ok(()) // Drop closes the connection.
}
```

The following examples reuse an open `driver` within the same resource scope.
Rust examples belong in a function that returns `Result`. Adapt error handling
to your application.

## Connection configuration

`Driver::connect(uri, username, password, database, timeout)`: pass `""` for the
server default and a `std::time::Duration`. `Driver::connect_with_options` takes
the same arguments plus `&ConnectOptions`. `Drop` closes the connection;
`close()` is also available.

| Setting | Behaviour |
|---|---|
| URI | `bolt://host:port` or, with the `tls` feature, `bolt+s://host:port`; omitted port is 7687 |
| Authentication | Required username and password; no credentials in the URI |
| Database | Empty database delegates selection to the server’s configured default |
| Named database | Supply an existing database name explicitly, for example `app` |
| Timeout | Examples use 30 seconds; it bounds each socket read and write |
| Result handling | Records are buffered in memory until the query completes |

The driver offers Bolt 5.0 to 5.4 and Bolt 4.4, and uses whichever the server
agrees. On 5.1 and newer it authenticates with a separate LOGON message after
HELLO. After connecting you can read:

| Method | Returns |
|---|---|
| `protocol_version()` | The agreed version, such as `"5.4"` or `"4.4"` |
| `server_agent()` | The server’s agent string from HELLO |
| `connection_id()` | The server’s id for this connection |
| `last_bookmark()` | The most recent bookmark from an autocommit query or COMMIT, if the server sent one |

URIs carry the host and port only. Query options, user information and the
routing schemes `neo4j://`, `neo4j+s://` and `neo4j+ssc://` are rejected with
`Error::Config`, as is `bolt+ssc://`.

Timeouts bound socket waits, not DNS lookup or the total duration of a query.
Mutable access enforces one operation at a time. Open separate drivers for
concurrent workers.

## Encrypted connections

A Galactus server started with `GDB_TLS_CERT` and `GDB_TLS_KEY` terminates TLS
itself on its usual Bolt port, and from then on requires it. Connect with
`bolt+s://` and the driver verifies the certificate against the host name in
the URI. Verification is always on, and the driver never falls back from
`bolt+s://` to plaintext.

Enable the `tls` feature to use `bolt+s://`. It adds
[rustls](https://github.com/rustls/rustls) with the ring crypto provider and
[rustls-platform-verifier](https://github.com/rustls/rustls-platform-verifier),
which trusts the operating system’s certificate store. A default build answers
`bolt+s://` with an `Error::Config` that names the feature.

For a private CA or a self-signed development server, give the driver a PEM file
with the certificates to trust. It replaces the platform store for that driver:

```rust
use galactus::{ConnectOptions, Driver};

let options = ConnectOptions::new().trusted_certificates("certs/galactus-ca.pem");
let mut driver = Driver::connect_with_options("bolt+s://db.internal:7687", "gdb",
    &std::env::var("GDB_PASSWORD")?, "", Duration::from_secs(30), &options)?;
```

Setting `trusted_certificates` on a `bolt://` URI is an `Error::Config`. A
certificate that cannot be verified gives `Error::Tls`.

## Queries and results

Bind data as parameters, never interpolate user input into Cypher:

```rust
let result = driver.execute_query(
    "CREATE (p:Person {name:$name, age:$age}) RETURN p.name AS name, p.age AS age",
    Map::from([("name".into(), "Ada".into()), ("age".into(), 37i64.into())]),
)?;
println!("{}", result.records[0]["name"].as_str().unwrap());
println!("{:?}", result.summary);
```

Results expose column keys, records indexed by column name, and server summary
metadata. Read a column with `result.records[0]["name"].as_str()`. Use unique
column aliases because records are maps. Summary fields can include the
database, query classification, timings, bookmark and update statistics; only
fields supplied by the server are present. A query without rows still returns
its summary.

Lists and maps can be nested. Values are recursively encoded and hydrated,
including graph properties and spatial envelopes. Use Cypher `LIMIT` or
application-level pagination for large result sets: results are collected in
full before `execute_query` returns.

To cap a result you don't control, use `execute_query_limited`. It pulls at
most that many records and discards the rest on the server. The flag it returns
is `true` when records were discarded:

```rust
let (result, truncated) = driver.execute_query_limited("MATCH (n) RETURN n", Map::new(), 100)?;
```

`set_transaction_timeout(Some(duration))` asks the server to stop each later
autocommit query, or transaction from its `BEGIN`, once the time runs out. The
server rolls back a stopped transaction. The socket timeout passed to `connect`
still applies, so keep it longer than the transaction timeout.

## Transactions

Queries outside an explicit transaction autocommit. Group related writes with
begin/commit; explicitly roll back when abandoning work:

```rust
driver.begin(false)?;
driver.execute_query("CREATE (:Person {name:$name})", Map::from([("name".into(), "Ada".into())]))?;
driver.execute_query("CREATE (:Person {name:$name})", Map::from([("name".into(), "Grace".into())]))?;
driver.commit()?;
// To deliberately abandon an open transaction: driver.rollback()?;
```

The transaction belongs to the connection. Keep its statements on the same
driver and finish the transaction before reusing the driver for unrelated work.
Closing a connection rolls back an unfinished transaction; it never commits
implicitly. The read-only argument requests a read transaction.

When a query inside a transaction fails with a database error, the server rolls
the transaction back and the driver marks it closed. Start a new transaction to
try again. A database error from `commit()` means the transaction did not
commit. A lost connection during COMMIT means the outcome is unknown: reconcile
the data before retrying a write.

## Native type mapping

`Value` contains native `i64`, `f64`, `bool`, `String`, `Vec<u8>`, `Vec<Value>`,
and `Map` variants, plus named graph, temporal, point and `Spatial` variants.
`From` handles ordinary conversions; `TryFrom` range-checks `u64`, `SystemTime`,
and standard durations. No serialisation or date library is required.

- Database integers are signed 64-bit; out-of-range integer inputs fail.
- Bytes stay binary; values do not pass through JSON.
- `DECIMAL` values arrive as strings holding their exact digits, for example
  `"12.30"`. Send a decimal as a string and convert it in Cypher with
  `decimal($amount)`.
- Temporal wrappers keep nanoseconds, offsets and named zones. `DateTime`
  stores local epoch seconds with its offset; the driver sends it in the form
  the agreed protocol uses.
- `ZonedDateTime` records whether its seconds are local or UTC
  (`ZonedDateTime::local` / `ZonedDateTime::utc`) and is sent in that basis.
  The server answers in its protocol’s basis (UTC on Bolt 5, local on 4.4), so
  a round trip returns the same instant and zone, possibly in the other basis.
- Calendar duration keeps months, days, seconds and nanoseconds separately.
- Nodes, relationships and paths have named result types. Every node and
  relationship carries `element_id` (relationships also `start_element_id`
  and `end_element_id`), which matches Cypher `elementId()` on Bolt 5 and 4.4
  alike. Bind ids or properties as parameters; graph entities are result-only.
- Unknown wire structures remain explicit structures and cannot be used as
  generic application-object serialisation.

Application models and UUIDs are converted explicitly to a supported value.
See the [complete type mapping](docs/BLUEPRINT.md) for precision and range
details. `encode_for(value, Protocol::Bolt5)` and `decode` expose the codec
for wire-level inspection.

## Spatial values

The driver supports `Point2D`, `Point3D`, and a native `Spatial` value covering
every shape family Galactus DB supports:

| Shape family | Geometry | Geography |
|---|---|---|
| Point / LineString / Polygon, including holes | Yes | Yes |
| MultiPoint / MultiLineString / MultiPolygon | Yes | Yes |
| GeometryCollection, including nested collections | Yes | Yes |
| XY, XYZ, and typed empty shapes | Yes | Yes |

`Spatial` preserves WKB bytes together with domain, SRID, coordinate layout and
model. Geometry uses `planar`; geography uses `greatCircle`. Neither domain nor
SRID is inferred by converting through GeoJSON.

```rust
let mut result = driver.execute_query(
    "RETURN spatial.fromWKT($wkt, {domain:$domain}) AS shape",
    Map::from([
        ("wkt".into(), "POLYGON((0 0,4 0,4 4,0 4,0 0))".into()),
        ("domain".into(), "geometry".into()),
    ]),
)?;
let shape = result.records.remove(0).remove("shape").unwrap();
driver.execute_query(
    "CREATE (:Region {name:$name, shape:spatial.fromMap($shape)})",
    Map::from([("name".into(), "Square".into()), ("shape".into(), shape)]),
)?;
```

Binding `Spatial` sends a lossless envelope. Use `spatial.fromMap($shape)` to
promote it into a database spatial value. `RETURN $shape` alone echoes the map;
it does not change the server’s value type. To import external bytes or text,
use `spatial.fromWKB`, `spatial.fromWKT`, or `spatial.fromGeoJSON` with
parameters. The server performs geometry and topology validation. See the
[spatial guide](docs/SPATIAL.md).

## Errors and connection lifecycle

| Variant | Meaning | Connection afterwards |
|---|---|---|
| `Error::Database { code, message, metadata }` | The server rejected the request | Reset and ready for the next query |
| `Error::Config` | Unusable URI or options, or `bolt+s://` without the `tls` feature | No connection was opened |
| `Error::Tls` | The TLS handshake or certificate check failed | Discarded |
| `Error::Io` | The socket failed or timed out | Discarded |
| `Error::Protocol` | The peer broke the protocol, or a parameter could not be sent | Discarded, except for parameter errors, which are raised before anything is sent |

After a database error the driver sends RESET, so the same driver runs the next
query straight away:

```rust
use galactus::{codes, Error};

match driver.execute_query("CREATE (:User {email:$email})", params) {
    Err(e) if e.code() == Some(codes::CONSTRAINT_VALIDATION_FAILED) => {
        // The email is taken; the driver is ready for the next query.
    }
    Err(e) if e.is_retryable() => { /* a transient error: try again later */ }
    other => { other?; }
}
```

Database errors classify their code: `classification()`, `category()` and
`title()` return its parts, and `is_client_error()`, `is_transient()`,
`is_database_error()` and `is_retryable()` answer the common questions.
`is_retryable()` is true for transient errors only; the driver itself never
retries. The `codes` module publishes the Galactus error codes as constants.
A failed login is terminal: the server closes the connection, and `connect`
returns the database error.

| Symptom | Check |
|---|---|
| Connection refused | Host, port, listener and network reachability |
| Authentication rejected | Database username and configured password |
| Database not found | Explicit database name; omit it to use the server default |
| `Error::Tls` | The server certificate’s host name and trust chain, or `trusted_certificates` |
| `Error::Config` naming `tls` | Enable the `tls` cargo feature |
| Driver is closed | The connection was discarded after an I/O, TLS or protocol failure; connect again |

Credentials and parameters are not logged by the driver. Keep them out of
application logs when reporting errors.

## Testing

Run from the repository root:

```sh
cargo test --offline
cargo test --offline --features tls
```

Fixtures and tests are self-contained. Without `GDB_TEST_URI`, live database
tests are skipped. For integration tests, use a fresh disposable server and set
`GDB_TEST_URI` and `GDB_TEST_PASSWORD`; tests create data and exercise
transactions in the server’s default database. See the
[test instructions](tests/README.md) for the full set of variables, including
TLS.

## Scope of this release

This release provides direct connections over Bolt 5.4 or 4.4, basic
authentication, verified TLS, parameterised queries, complete results with
summaries, explicit transactions, recovery after database errors, error
classification, native type mapping and full spatial interchange.

Each driver owns one connection. Connection pools, routing discovery, sessions
that send bookmarks, managed retry callbacks, lazy cursors, cancellation and
asynchronous APIs belong to later releases. Messages are capped at 64 MiB and
nesting at 64 levels; a complete result can be larger than one message. See
[the shared blueprint](docs/BLUEPRINT.md) and the
[implementation overview](docs/OVERVIEW.md) for the precise contract.

## Feedback and contribution

Use [GitHub issues](https://github.com/galactusdb/galactus-db-rust-driver/issues)
for reproducible bugs and focused feature requests. Include runtime and OS
versions, the driver commit, the expected behaviour and a small reproduction
with credentials and sensitive data removed. Keep the default build free of
dependencies and run the relevant tests before opening a pull request against
`main`.

Visit [galactusdb.com](https://galactusdb.com) for Galactus DB, or browse the
[other language drivers](https://galactusdb.com/product/drivers).

## Licence

Apache License 2.0. See [LICENSE](LICENSE).
