# Galactus DB drivers

Seven native **database drivers**, with a shared API and zero third-party runtime
packages in their default builds. Version **0.2.0** is early access. All seven
source repositories are public under [galactusdb](https://github.com/galactusdb),
with `main` as the default branch. Registry packages have not been published;
install from source using the language Quickstart and pin a reviewed commit. Each
driver speaks **Bolt 5.0 to 5.4** with a **Bolt 4.4** fallback, using its host
language directly with neither a helper runtime nor the server source at runtime.

Use **driver** for package names and public APIs. Use **SDK** for a future wider
bundle containing drivers, administration tools, examples, and integrations.

| Language | Folder | Public import / namespace | Runtime package dependencies |
|---|---|---|---|
| C# | [galactus-db-csharp-driver](https://github.com/galactusdb/galactus-db-csharp-driver) | `Galactus`; package `Galactus.Driver` | 0 |
| Rust | [galactus-db-rust-driver](https://github.com/galactusdb/galactus-db-rust-driver) | `galactus` | 0 (the optional `tls` feature adds rustls) |
| Java | [galactus-db-java-driver](https://github.com/galactusdb/galactus-db-java-driver) | `db.galactus` | 0 |
| Go | [galactus-db-go-driver](https://github.com/galactusdb/galactus-db-go-driver) | `galactus` | 0 |
| Node / TypeScript | [galactus-db-node-driver](https://github.com/galactusdb/galactus-db-node-driver) | `galactus-db-node-driver` | 0 |
| C++ | [galactus-db-cpp-driver](https://github.com/galactusdb/galactus-db-cpp-driver) | `galactus`; CMake `Galactus::Driver` | 0 (the optional `GALACTUS_WITH_TLS` uses the platform TLS library) |
| Python | [galactus-db-python-driver](https://github.com/galactusdb/galactus-db-python-driver) | `galactus` | 0 |

See the [driver catalogue](https://galactusdb.com/product/drivers) for repository
links and runtime requirements.

## Shared developer experience

1. Open a driver with a URI, username, password, and optional database/timeout.
2. Execute parameterised Cypher using `execute_query` / `executeQuery` /
   `ExecuteQuery`, following the language's conventions.
3. Read `records` by column name and retain `keys` and `summary` metadata.
4. Use `begin`, `commit`, and `rollback` for explicit transactions.
5. Close/dispose the driver; closing an open transaction rolls it back on the
   server. Rust and C++ also close on destruction.
6. Read the agreed `protocol_version`, `server_agent`, `connection_id` and
   `last_bookmark` from the driver.

No generated models, ORM, schema registration, JSON conversion, or special
integer class is needed for ordinary values. Parameter values are encoded
recursively, and returned values are hydrated recursively, including spatial
values inside graph properties and nested containers.

Database integers are signed 64-bit. Node returns **native `bigint`**, while
JavaScript `number` parameters encode as floating point. Other languages use
their native signed integer representations. Out-of-range integers fail rather
than being truncated. Unknown wire structures remain explicit structures.

See [the blueprint and mapping table](BLUEPRINT.md) for exact semantics and
[the spatial contract](SPATIAL.md) for geometry/geography handling.

## All supported spatial types

Every driver has `Point2D`, `Point3D`, and a native `Spatial` value preserving
WKB bytes plus domain, SRID, XY/XYZ layout, and model. This covers Point,
LineString, Polygon with holes, MultiPoint, MultiLineString, MultiPolygon,
GeometryCollection, nested collections, and typed empties in both geometry and
geography.

The database's versioned envelope is used without coordinate rewriting or
conversion through GeoJSON. Bind a `Spatial` value directly, then use
`spatial.fromMap($shape)` in Cypher to turn that envelope into a database shape.
This explicit promotion is required by the existing server protocol.

## Scope of 0.2

These are small **single-connection, eager-result drivers**. One connection is
owned by each driver. Use a separate driver for each concurrent unit of work,
especially transactions. Go, Java and C# serialize calls; Node rejects overlapping
operations; Python and C++ must not be shared concurrently; Rust requires mutable
access. There is no connection pool, routing discovery, automatic retry,
managed transaction callback, session bookmark chaining, lazy cursor, ORM, or
async API outside Node yet. Session re-authentication, telemetry and routing
messages are not sent.

The handshake offers Bolt 5.4 down to 5.0, then 4.4. From 5.1 the drivers send
`bolt_agent` in HELLO and authenticate with a separate LOGON. On 5.x, offset and
named-zone datetimes use UTC-based seconds (`I`/`i`) and graph values carry
element ids; on 4.4 the drivers fill element ids from the integer id, which is
what Galactus's `elementId()` returns. Every decoder accepts both forms.

`bolt://` is direct TCP. `bolt+s://` is verified TLS with the URI host as the
server name; there is no skip-verify option and no driver falls back to
plaintext. A Galactus server started with `GDB_TLS_CERT` and `GDB_TLS_KEY`
terminates TLS itself on the Bolt port and then requires it. Python, Node, Go,
Java and C# use their standard TLS libraries. Rust needs its `tls` cargo feature
(rustls with the platform verifier) and C++ its `GALACTUS_WITH_TLS` option;
their default builds reject `bolt+s://` with an error naming the feature. Every
driver accepts an optional trusted-certificate PEM file that replaces the
platform store, for a private CA or self-signed development server.

Messages are limited to 64 MiB and nesting to 64 levels. Query results are eager
and may collectively exceed one message; use Cypher `LIMIT` for large queries.
I/O timeouts default to 30 seconds. They bound socket waits (Go uses a deadline
for the complete operation), not total DNS resolution or every query's total
elapsed time. After a database FAILURE the driver sends RESET, marks any open
transaction closed (the server rolls it back) and raises the error; the same
driver is then ready for the next query. Protocol, I/O and TLS failures, a
failed RESET and failed authentication discard the connection; open a new
driver after those. A transport failure during COMMIT has an **unknown commit
outcome**; the drivers never automatically replay a write.

Database errors keep the server code and message and add its classification,
category and title, `is_client_error`, `is_transient`, `is_database_error` and
`is_retryable` (transient codes only). Each driver publishes the Galactus codes
as constants.

Returned graph entities are result-only database types; bind their IDs or
properties rather than the entities. `DECIMAL` values arrive as strings holding
their exact digits; C#, Java and Python also send their standard decimal types,
and other languages send strings for `decimal($param)`. Arbitrary application
objects and UUIDs are not implicitly converted. Convert them explicitly to a
supported representation. Use unique result aliases when reading record maps.

## Verification

[Test instructions](../tests/README.md) describe the shared golden wire fixtures,
malformed-input tests, and isolated real-server runner. Every language has been
compiled/executed locally against a freshly built Galactus server. The common
spatial fixture has 57 cases per language, including both domains, XY/XYZ,
empties, holes, nested collections, and antimeridian geography.

The live runner exercises each driver over Bolt 5.4, over Bolt 4.4
(`GDB_BOLT_MAX_VERSION=4.4`) and over verified TLS against a throwaway test CA.
Platform validation so far is Windows. Linux/macOS builds, pooling under load,
cancellation and production hardening remain release work.

## Further reading

See the [driver blueprint](BLUEPRINT.md) for the API and type mapping contract,
and [spatial mapping](SPATIAL.md) for geometry and geography interchange.
Visit [galactusdb.com](https://galactusdb.com) for the database and product guides.
