# Galactus DB drivers

Seven native **database drivers**, with a shared API and zero third-party runtime
packages. This is the local, experimental **0.1.0** implementation; packages have
not been published. Each driver speaks the existing Galactus **Bolt 4.4** protocol
directly. None wraps a Neo4j driver, launches another runtime, or needs the server
source at runtime.

Use **driver** for package names and public APIs. Use **SDK** for a future wider
bundle containing drivers, administration tools, examples, and integrations.

| Language | Folder | Public import / namespace | Runtime package dependencies |
|---|---|---|---|
| C# | [galactus-db-csharp-driver](https://github.com/galactusdb/galactus-db-csharp-driver) | `Galactus`; package `Galactus.Driver` | 0 |
| Rust | [galactus-db-rust-driver](https://github.com/galactusdb/galactus-db-rust-driver) | `galactus` | 0 |
| Java | [galactus-db-java-driver](https://github.com/galactusdb/galactus-db-java-driver) | `db.galactus` | 0 |
| Go | [galactus-db-go-driver](https://github.com/galactusdb/galactus-db-go-driver) | `galactus` | 0 |
| Node / TypeScript | [galactus-db-node-driver](https://github.com/galactusdb/galactus-db-node-driver) | `galactus-db-node-driver` | 0 |
| C++ | [galactus-db-cpp-driver](https://github.com/galactusdb/galactus-db-cpp-driver) | `galactus`; CMake `Galactus::Driver` | 0 |
| Python | [galactus-db-python-driver](https://github.com/galactusdb/galactus-db-python-driver) | `galactus` | 0 |

## Shared developer experience

1. Open a driver with a URI, username, password, and optional database/timeout.
2. Execute parameterised Cypher using `execute_query` / `executeQuery` /
   `ExecuteQuery`, following the language's conventions.
3. Read `records` by column name and retain `keys` and `summary` metadata.
4. Use `begin`, `commit`, and `rollback` for explicit transactions.
5. Close/dispose the driver; closing an open transaction rolls it back on the
   server. Rust and C++ also close on destruction.

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

## Scope of 0.1

These are small **single-connection, eager-result drivers**. One connection is
owned by each driver. Use a separate driver for each concurrent unit of work,
especially transactions. Go, Java and C# serialize calls; Node rejects overlapping
operations; Python and C++ must not be shared concurrently; Rust requires mutable
access. There is no connection pool, routing discovery, automatic retry,
managed transaction callback, lazy cursor, ORM, or async API outside Node yet.

`bolt://` is direct TCP. Python, Node, Go, Java and C# also support verified
`bolt+s://` using system TLS facilities. Rust and C++ reject TLS URIs and require
an external TLS tunnel when encryption is needed. No driver silently downgrades
TLS. The current Galactus listener is plaintext; TLS requires a terminating proxy.

Messages are limited to 64 MiB and nesting to 64 levels. Query results are eager
and may collectively exceed one message; use Cypher `LIMIT` for large queries.
I/O timeouts default to 30 seconds. They bound socket waits (Go uses a deadline
for the complete operation), not total DNS resolution or every query's total
elapsed time. Protocol/I/O/database failures discard the connection. Open a new
driver after failure. A failed COMMIT can have an **unknown commit outcome**;
the drivers never automatically replay a write.

Returned graph entities are result-only database types; bind their IDs or
properties rather than the entities. Arbitrary application objects, UUIDs and
decimal types are not implicitly converted. Convert them explicitly to a
supported representation. Use unique result aliases when reading record maps.

## Verification

[Test instructions](../tests/README.md) describe the shared golden wire fixtures,
malformed-input tests, and isolated real-server runner. Every language has been
compiled/executed locally against a freshly built Galactus server. The common
spatial fixture has 57 cases per language, including both domains, XY/XYZ,
empties, holes, nested collections, and antimeridian geography.

Platform validation so far is Windows. TLS interoperability, Linux/macOS builds,
pooling under load, cancellation and production hardening remain release work.

## Design references

The API follows the familiar parameterised-query, records, summary, and explicit
transaction pattern described in the [Neo4j Python API](https://neo4j.com/docs/api/python-driver/current/api.html).
The wire implementation follows [PackStream](https://neo4j.com/docs/bolt/current/packstream/)
and the version-specific [Bolt structure semantics](https://neo4j.com/docs/bolt/current/bolt/structure-semantics/).
Unlike the custom integer type documented in the [Neo4j JavaScript mapping](https://neo4j.com/docs/javascript-manual/current/data-types/),
this Node driver uses built-in `bigint`. Galactus source and its Bolt 4.4 tests
are authoritative where newer Bolt versions differ.
