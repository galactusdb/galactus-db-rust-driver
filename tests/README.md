# galactus-db-rust-driver tests

[Galactus DB website](https://galactusdb.com) · [Driver Quickstart](../README.md)

## Quickstart

Run these commands from the repository root:

```sh
cargo test --offline
cargo test --offline --features tls
```

The fixtures are included in `tests/fixtures`; no sibling repository is needed.
The tests use standard runtime/test facilities, with no test framework packages.
The wire fixtures cover integer boundaries, UTF-8, temporal values, graph values,
points, malformed inputs and unknown structures. `values.tsv` round-trips in
Bolt 4.4 form and `bolt5.tsv` (element ids, `I`/`i`/`f` datetimes) in Bolt 5
form; each `datetime_pairs.tsv` row holds the same offset datetime as `F` and
as `I`, which must decode equal and encode to the column of each protocol.
Unit tests also check URI and option errors, error classification, and that the
default build rejects `bolt+s://` with an error naming the `tls` feature. The spatial fixture contains
57 geometry/geography cases, including XY/XYZ, empties, holes, multipart shapes,
nested collections, and antimeridian geography.

### Live database tests

Without `GDB_TEST_URI`, live tests are skipped. To include them, start a **fresh,
disposable** Galactus database and set these environment variables before running
the same test command:

```sh
export GDB_TEST_URI='bolt://127.0.0.1:7687'
export GDB_TEST_PASSWORD='your-disposable-database-password'
```

```powershell
$env:GDB_TEST_URI = 'bolt://127.0.0.1:7687'
$env:GDB_TEST_PASSWORD = 'your-disposable-database-password'
```

Optional variables:

- `GDB_TEST_EXPECT_BOLT`: the version the driver must agree, `5.4` or `4.4`
  (start the server with `GDB_BOLT_MAX_VERSION=4.4` for the second).
- `GDB_TEST_CA_FILE`: a PEM CA for a `bolt+s://` URI, passed as the
  `trusted_certificates` option. Run with `--features tls`; the test then also
  checks that the same server is rejected with `Error::Tls` without the option,
  and that the option on `bolt://` is `Error::Config`.

Tests use username `gdb` and the server's default database. They create nodes
labelled `DriverRust`, `DriverRustGraph`, `DriverRustTx` and `DriverRustUnique`
and a unique constraint on `DriverRustUnique(key)`, and check protocol
agreement, value and datetime round trips, element ids, decimals as strings,
commit/rollback, recovery after a failed query and after a failure inside a
transaction, constraint error codes, transaction timeouts and rejected
authentication. Do not point them at valuable data. Use a fresh database for
each run because commit tests leave nodes.

The shared runner in the parent `galactus-db-drivers` folder runs the suite in
three passes against disposable servers: Bolt 5.4, Bolt 4.4 and verified TLS.
See [implementation scope](../docs/OVERVIEW.md#scope-of-02).

Database information: [Galactus DB website](https://galactusdb.com).
