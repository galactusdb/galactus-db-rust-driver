# galactus-db-rust-driver tests

[Galactus DB website](https://galactusdb.com) · [Driver Quickstart](../README.md)

## Quickstart

Run these commands from the repository root:

```sh
cargo test --offline
```

The fixtures are included in `tests/fixtures`; no sibling repository is needed.
The tests use standard runtime/test facilities, with no test framework packages.
The wire fixtures cover integer boundaries, UTF-8, temporal values, graph values,
points, malformed inputs and unknown structures. The spatial fixture contains
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

Tests use username `gdb`, the server's default database, create test-labelled nodes and exercise
commit/rollback, errors and rejected authentication. Do not point them at valuable
data. Reset the disposable database between runs because commit tests leave nodes.
All seven drivers were validated together against a fresh local Galactus server;
see [implementation scope](../docs/OVERVIEW.md#scope-of-01).

Database information: [Galactus DB website](https://galactusdb.com).
