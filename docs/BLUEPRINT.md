# Driver blueprint, version 0.2

Version 0.2.0 speaks Bolt 5.0 to 5.4 with a Bolt 4.4 fallback, verifies TLS in
every language, recovers the connection after a database error, and gives
database errors a classification applications can act on. The 0.1 API keeps
working: everything below adds to it.

## Names and transport

Folder names are `galactus-db-{language}-driver`. Public symbols use the host
language's naming conventions. The consistent operations are connection,
parameterised query, begin, commit, rollback, and close. A driver owns one
authenticated connection and one optional explicit transaction.

### Handshake

Send the preamble `6060b017`, then four proposals: `00040405` (5.4, accepting
down to 5.0), `00000404` (4.4), and two zero proposals. Accept a response of
`0000mm05` with `mm` from `00` to `04`, or exactly `00000404`; anything else is a
protocol error. Keep the agreed version on the connection and expose it as a
read-only `protocol_version` / `protocolVersion` / `ProtocolVersion` string
(`"5.4"`, `"4.4"`) on the driver. Also expose the server agent and
`connection_id` from the HELLO SUCCESS (`server_agent` / `serverAgent` /
`ServerAgent`, and `connection_id` / `connectionId` / `ConnectionId`).

A server started with `GDB_BOLT_MAX_VERSION=4.4` answers `00000404`; the same
driver then uses the 4.4 message and value forms.

### Authentication

- Bolt 4.4 and 5.0: one HELLO carries `user_agent`, `scheme: basic`,
  `principal` and `credentials`.
- Bolt 5.1 and newer: HELLO carries `user_agent` and `bolt_agent`, and no
  credentials. Read its SUCCESS, then send LOGON (`0x6A`) with one map:
  `scheme: basic`, `principal`, `credentials`. Read its SUCCESS. Do not pipeline
  the two.
- `bolt_agent` is a map: `product` (`galactus-db-{language}-driver/0.2.0`),
  `language` (for example `Python/3.12.1`), `language_details` (runtime or
  compiler name, optional) and `platform` (OS and architecture, optional). Send
  it from 5.1 on (not in a 5.0 HELLO); 5.3 makes it required.
- `user_agent` is `galactus-db-{language}-driver/0.2.0`.

A failed authentication is terminal: the server closes the connection. Raise
the database error with the server's code and discard the connection.
Credentials are separate arguments, never URI userinfo. No driver logs
credentials or query parameters. Reject unsupported URI schemes, all URI query
options, and routing URIs.

Do not send TELEMETRY, LOGOFF or ROUTE. Session re-authentication, routing and
telemetry are not part of 0.2.

### Running queries

Autocommit: `RUN(query, params, {db})`, consume SUCCESS fields, then `PULL {n:-1}`
until terminal SUCCESS. Honour `has_more` if supplied. An explicit transaction
sends `BEGIN {db,mode}`; subsequent RUN extras are empty; COMMIT/ROLLBACK have
zero fields. An empty or omitted database setting omits `db` from RUN/BEGIN
metadata, letting the server select its configured default. Go and Rust accept
an explicit empty string for this setting. A nonempty database name is sent
unchanged; the driver does not create the database.
Graph IDs are local database identities, not application keys.

The terminal SUCCESS of an autocommit PULL and the SUCCESS of COMMIT may carry
a `bookmark` string. Keep it in the result summary metadata and expose the most
recent one as `last_bookmark` / `lastBookmark` / `LastBookmark` on the driver.
The driver does not send bookmarks in 0.2.

Only one operation may use a connection at a time. Do not pipeline requests.

### Recovering after a database error

When the server answers RUN, PULL, BEGIN, COMMIT or ROLLBACK with FAILURE, the
connection itself is healthy. Send RESET (`0x0F`, no fields) and read messages
until its SUCCESS, skipping any IGNORED. The server rolls back an open
transaction on RESET, so mark the driver's transaction as closed. Then raise the
database error. The driver stays usable for the next query.

Discard the connection, as in 0.1, when:

- reading or writing fails (I/O, timeout, TLS),
- the peer breaks the protocol (malformed message, unexpected response),
- RESET itself does not end in SUCCESS (raise the original database error),
- an IGNORED arrives anywhere other than while reading RESET's reply,
- authentication fails.

A FAILURE answering COMMIT means the transaction did not commit. A transport
failure during COMMIT is still an **unknown commit outcome**; never retry it
automatically.

### TLS

`bolt+s://` uses verified TLS with the host name from the URI as the server name
(SNI and certificate check). `bolt://` is plaintext. Never fall back from one
to the other. Reject `bolt+ssc://` and every routing scheme (`neo4j://`,
`neo4j+s://`, `neo4j+ssc://`).

A Galactus server terminates TLS itself when started with `GDB_TLS_CERT` and
`GDB_TLS_KEY`, on the same port as plaintext. Once a certificate is configured
the server requires TLS unless `GDB_TLS_REQUIRED=false`.

Trust comes from the platform's store by default. Every driver also accepts an
optional **trusted certificate file**: a path to one or more PEM certificates
that replaces the platform store for that driver (`trusted_certificates` /
`trustedCertificates` / `TrustedCertificates`, following the language's option
style). Setting it on a `bolt://` URI is a configuration error. This is what a
private CA or a self-signed development server needs; there is no option to
skip verification.

| Language | Provider | Default build |
|---|---|---|
| Python | `ssl` standard library | included |
| Node | `node:tls` | included |
| Go | `crypto/tls` | included |
| Java | JSSE (`javax.net.ssl`) | included |
| C# | `SslStream` | included |
| Rust | `rustls` with the platform verifier, behind the `tls` cargo feature | off: the default build keeps zero dependencies and rejects `bolt+s://` with an error naming the feature |
| C++ | SChannel on Windows, OpenSSL on Linux and macOS, behind the CMake option `GALACTUS_WITH_TLS` (macro `GALACTUS_WITH_TLS`) | off: the default build keeps zero dependencies and rejects `bolt+s://` with an error naming the option |

## Value mapping

| Database value | C# | Rust | Java | Go | Node | C++ | Python |
|---|---|---|---|---|---|---|---|
| Null | `null` | `Value::Null` / `Option` input | `null` | `nil` | `null` | null `Value` | `None` |
| Boolean | `bool` | `bool` | `Boolean` | `bool` | `boolean` | `bool` | `bool` |
| Integer | `long` | `i64` | `Long` | `int64` | `bigint` | `int64_t` | `int` |
| Float | `double` | `f64` | `Double` | `float64` | `number` | `double` | `float` |
| String | `string` | `String` | `String` | `string` | `string` | UTF-8 `string` | `str` |
| Bytes | `byte[]` | `Vec<u8>` | `byte[]` | `[]byte` | `Buffer` / `Uint8Array` | `Bytes` | `bytes` |
| List | `List<object?>` | `Vec<Value>` | `List<Object>` | `[]any` | `Array` | `List` | `list` |
| Map | `Dictionary<string,object?>` | `Map` | `Map<String,Object>` | `map[string]any` | own-key object | `Map` | `dict` |
| Date | `DateOnly`* | `Date` | `LocalDate`* | `Date` | `LocalDate` | `Date` | `datetime.date`* |
| Local time | `TimeOnly`* | `LocalTime` | `LocalTime` | `LocalTime` | `LocalTime` | `LocalTime` | `datetime.time`* |
| Offset time | `OffsetTime` | `Time` | `OffsetTime` | `Time` | `Time` | `Time` | aware `datetime.time`* |
| Local datetime | `DateTime` (Unspecified)* | `LocalDateTime` | `LocalDateTime`* | `LocalDateTime` | `LocalDateTime` | `LocalDateTime` | naive `datetime.datetime`* |
| Offset datetime | `DateTimeOffset`* | `DateTime` | `OffsetDateTime`* | `time.Time` | `DateTime` | `DateTime` | aware `datetime.datetime`* |
| Named-zone datetime | `ZonedDateTime` | `ZonedDateTime` | `ZonedDateTimeValue` | `ZonedDateTime` | `ZonedDateTime` | `ZonedDateTime` | `ZonedDateTime` |
| Duration | `Duration` | `Duration` | `Types.Duration` | `Duration` | `Duration` | `Duration` | `Duration` |
| Graph | named records | named `Value` variants | named classes | named structs | named classes | named structures via `as<T>()` | named tuples |
| Legacy point | `Point2D/3D` | `Point2D/3D` | `Point2D/3D` | `Point2D/3D` | `Point2D/3D` | `Point2D/3D` | `Point2D/3D` |
| Geometry/geography | `Spatial` | `Spatial` | `Spatial` | `Spatial` | `Spatial` | `Spatial` | `Spatial` |

`*` If a value cannot fit the native range or precision, return a lossless
wrapper instead (Java falls back to `Types.Structure` for an out-of-range
temporal). Python's native temporal precision is microseconds; .NET's is 100 ns.
Java, Go and the explicit types retain nanoseconds. Named-zone values retain
the IANA name and local seconds in wrappers to avoid resolving DST overlaps
silently. Python/Java native aware datetime inputs encode using their resolved
offset; use `ZonedDateTime` / `ZonedDateTimeValue` when the zone ID must survive.

The wrappers use signed epoch **days**, nanoseconds of local day, epoch
**seconds**, nanoseconds of second, offset seconds, or IANA zone ID as appropriate.
A database duration has independent months, days, seconds, and nanoseconds;
never assume a month has 30 days or collapse calendar duration to elapsed time.

### Datetimes on Bolt 4.4 and Bolt 5

| Tag | Fields | `seconds` means | Sent by the server on |
|---|---|---|---|
| `F` (`0x46`) | seconds, nanos, offset_seconds | **local** epoch seconds; UTC is `seconds - offset_seconds` | 4.4 |
| `f` (`0x66`) | seconds, nanos, zone_id | **local** epoch seconds in that zone | 4.4 |
| `I` (`0x49`) | seconds, nanos, offset_seconds | **UTC** epoch seconds | 5.x |
| `i` (`0x69`) | seconds, nanos, zone_id | **UTC** epoch seconds | 5.x |

Decoders accept all four tags on any protocol version and apply each tag's own
meaning. An offset datetime decoded from `F` and from `I` for the same instant
and offset is the same value (`tests/fixtures/datetime_pairs.tsv`).

Encoders write offset datetimes as `I` on a 5.x connection and as `F` on 4.4;
the conversion is exact arithmetic. The codec therefore takes the agreed
protocol as an input (a flag or mode), and encoding happens only after the
handshake.

A named-zone datetime cannot move between local and UTC seconds without a time
zone database, which not every language has. `ZonedDateTime` (Java:
`ZonedDateTimeValue`) therefore records which basis its seconds use, local or
UTC, with a constructor for each. A value decoded from `f` is local-based and
one from `i` is UTC-based. The encoder writes the value's own basis: `f` for
local, `i` for UTC, on either protocol; the Galactus server accepts both tags
on every version. Two wrappers are equal only when basis, seconds, nanos and
zone match. The server answers in its protocol's own basis (local `f` on 4.4,
UTC `i` on 5.x), so `RETURN $v` gives back the same instant and zone but not
necessarily the same basis; compare instants in tests. Languages with a time zone database (Java, C#, Go, Python, Node)
may offer a conversion helper; none is required.

### Graph values on Bolt 5

| Struct | Bolt 4.4 fields | Bolt 5 fields |
|---|---|---|
| Node `N` | id, labels, properties | + element_id |
| Relationship `R` | id, start, end, type, properties | + element_id, start_element_id, end_element_id |
| UnboundRelationship `r` | id, type, properties | + element_id |

Decoders accept both lengths. Every node, relationship and unbound
relationship exposes `element_id` (and a relationship `start_element_id` /
`end_element_id`) as a string, following the language's naming. On 4.4 the
driver fills them with the decimal text of the integer id, which is what
Galactus's `elementId()` returns. The golden encoder writes the form of the
protocol it is given, so `tests/fixtures/bolt5.tsv` round-trips in Bolt 5 mode
and `values.tsv` in 4.4 mode. Graph values remain result-only: they are still
rejected as query parameters.

### Decimals

Galactus returns `DECIMAL` values as strings holding their exact digits; read
them as strings. Where the language has a standard decimal type, the encoder
also accepts it and sends its exact plain text (no exponent):

- C#: `decimal`, as `ToString(CultureInfo.InvariantCulture)`.
- Java: `BigDecimal`, as `toPlainString()`.
- Python: `decimal.Decimal`, as `format(value, 'f')`; NaN and infinities are
  rejected.

Queries turn the string back into a decimal with `decimal($param)`. Go, Rust,
Node and C++ have no standard decimal type; applications send strings.

Additional native input conversions:

- Python: `date`, `time`, `datetime`, `timedelta`; bytearray/memoryview and tuple.
- C#: DateOnly, TimeOnly, UTC/Unspecified DateTime, DateTimeOffset, TimeSpan.
  Local DateTime must be converted explicitly to DateTimeOffset.
- Java: java.time types, Instant, ZonedDateTime (resolved offset), Duration,
  Period, arrays and collections; checked BigInteger within signed 64-bit.
- Go: signed integer widths, checked unsigned widths, float32, typed slices,
  string-keyed maps, time.Time, and time.Duration.
- Node: native Date input becomes an offset-zero DateTime; output keeps a
  nanosecond wrapper. `number` is FLOAT; use `42n` for INTEGER. Construct large
  integers from bigint/string, not an already-rounded JavaScript number.
- Rust: `From` for primitives, `Option<T>`, maps/lists and named values;
  checked `TryFrom` for u64, SystemTime, and std Duration. Explicit conversion
  back to SystemTime rejects values outside its range.
- C++: checked integral constructors, standard strings/containers, named values,
  system_clock::time_point input and checked `to_system_time(DateTime)` output.

Returned graph entities, paths and unknown structures are not accepted as query
parameters because the server cannot promote them. Bind node IDs/properties.
`Structure` is retained for low-level wire inspection, not a generic object
serialization escape hatch. General application-object/record mapping belongs
in an optional adapter; it must not add dependencies to the core driver.

## Error and resource contract

Database errors retain the server code and message (and metadata). Protocol
errors, invalid UTF-8, malformed containers, truncated frames, invalid versions,
and unsupported parameter values fail explicitly. Encoders reject signed
integer overflow and non-string map keys. Decoders reject trailing bytes,
duplicate map keys and nesting above 64. Messages are capped at 64 MiB.

Validate/encode a complete request before writing it. A parameter that fails
validation raises before anything is sent and leaves the connection usable.
After a database
FAILURE, reset and keep the connection (see "Recovering after a database
error"). On protocol, I/O or TLS failure discard it. Treat a transport failure
during COMMIT as unknown outcome, never a safe retry.

### Error classification

A server code has four dot-separated parts: `{Vendor}.{Classification}.{Category}.{Title}`,
where the vendor is `Galactus` or `Neo` and the classification is
`ClientError`, `TransientError` or `DatabaseError`. The database error type
keeps `code` and `message` unchanged and adds, in the language's naming:

- `classification`, `category` and `title`: the parts of the code (empty or
  null when the code does not have four parts);
- `is_client_error`, `is_transient` and `is_database_error`;
- `is_retryable`: true for `TransientError` codes only. The driver never
  retries by itself.

Each driver also publishes the codes below as constants (Python module
constants, a C# static class, Java/Go/Rust/C++ constants, a frozen Node object),
named after the title in the language's constant style (`ErrorCodes.TypeError`
in C#, `ErrorCodes.TYPE_ERROR` in Java, `codes.TYPE_ERROR` in Python, `galactus.CodeTypeError` in Go, `codes::TYPE_ERROR` in Rust,
`galactus::codes::type_error` in C++), so applications
compare against a name rather than a string. Codes the server adds later still work as plain strings.

| Constant | Code |
|---|---|
| ConstraintValidationFailed | `Galactus.ClientError.Schema.ConstraintValidationFailed` |
| TypeError | `Galactus.ClientError.Statement.TypeError` |
| ArgumentError | `Galactus.ClientError.Statement.ArgumentError` |
| SqlTooManyRows | `Galactus.ClientError.Statement.SqlTooManyRows` |
| SqlCastFailed | `Galactus.ClientError.Statement.SqlCastFailed` |
| TableTooLarge | `Galactus.ClientError.Statement.TableTooLarge` |
| UnsupportedFeature | `Galactus.DatabaseError.Storage.UnsupportedFeature` |
| ProcedureNotFound | `Galactus.ClientError.Procedure.ProcedureNotFound` |
| LicenseRestricted | `Galactus.ClientError.License.Restricted` |
| ReadOnlyReplica | `Galactus.ClientError.Database.ReadOnlyReplica` |
| TransactionTimedOut | `Galactus.ClientError.Transaction.TimedOut` |
| TransactionTerminated | `Galactus.ClientError.Transaction.Terminated` |
| SnapshotTooOld | `Galactus.TransientError.Transaction.SnapshotTooOld` |
| Forbidden | `Galactus.ClientError.Security.Forbidden` |
| SyntaxError | `Neo.ClientError.Statement.SyntaxError` |
| SemanticError | `Neo.ClientError.Statement.SemanticError` |
| Unauthorized | `Neo.ClientError.Security.Unauthorized` |
| DatabaseNotFound | `Neo.ClientError.Database.DatabaseNotFound` |

Keep the constants inside a codes namespace (in Go, a `Code` prefix) so names such
as `TypeError` do not clash with the language's own. A configuration error (for example a trusted
certificate file on a `bolt://` URI) raises the language's usual argument or
configuration error before connecting.

## Follow-on releases

Keep this baseline stable while adding bounded connection pools, sessions that
send bookmarks, fetch-size cursors, cancellation, async variants, retry with an
idempotent transaction callback, and optional application-object mapping. Add
these behind explicit features or dependencies, with shared behaviour tests. Do
not advertise cluster routing, full driver compatibility, or production
readiness based solely on speaking Bolt 5.
