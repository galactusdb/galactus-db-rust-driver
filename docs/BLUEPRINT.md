# Driver blueprint, version 0.1

## Names and transport

Folder names are `galactus-db-{language}-driver`. Public symbols use the host
language's naming conventions. The consistent operations are connection,
parameterised query, begin, commit, rollback, and close. A driver owns one
authenticated connection and one optional explicit transaction.

Offer only Bolt 4.4: preamble `6060b017`, proposal `00000404`, three zero
proposals. Require exactly `00000404` in the server's response. Send HELLO with
`user_agent`, `scheme: basic`, `principal`, and `credentials`. Credentials are
separate arguments, never URI userinfo. No driver logs credentials or query
parameters. Reject unsupported URI schemes, URI query options, and routing URIs.

Autocommit: `RUN(query, params, {db})`, consume SUCCESS fields, then `PULL {n:-1}`
until terminal SUCCESS. Honour `has_more` if supplied. An explicit transaction
sends `BEGIN {db,mode}`; subsequent RUN extras are empty; COMMIT/ROLLBACK have
zero fields. An empty or omitted database setting omits `db` from RUN/BEGIN
metadata, letting the server select its configured default. Go and Rust accept
an explicit empty string for this setting. A nonempty database name is sent
unchanged; the driver does not create the database.
Graph IDs are local database identities, not application keys.

Only one operation may use a connection at a time. Do not pipeline requests.
Discard failed connections rather than exposing a half-consumed stream. This
avoids needing a pool or a RESET recovery policy in the first version.

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

The wrappers use signed epoch **days**, nanoseconds of local day, local epoch
**seconds**, nanoseconds of second, offset seconds, or IANA zone ID as appropriate.
Bolt 4.4 datetime tags `F`/`f` use **local epoch seconds**. Offset datetime's UTC
instant is `seconds - offset_seconds`. Do not apply Bolt 5 UTC semantics to these
tags. A database duration has independent months, days, seconds, and nanoseconds;
never assume a month has 30 days or collapse calendar duration to elapsed time.

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

Validate/encode a complete request before writing it. On protocol, I/O or server
failure discard the connection. Treat a transport failure during COMMIT as
unknown outcome, never a safe retry. Any future retry API must require an
idempotent transaction callback and classify retryability explicitly.

## Follow-on releases

Keep this baseline stable while adding bounded connection pools, sessions with
bookmarks, fetch-size cursors, cancellation, async variants, optional Rust/C++
TLS providers, and optional application-object mapping. Add these behind
explicit features/dependencies, with shared behaviour tests. Do not advertise
cluster routing, full driver compatibility, or production readiness based solely
on speaking Bolt 4.4.
