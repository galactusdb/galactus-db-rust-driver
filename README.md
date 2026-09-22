# Galactus Rust driver

[Galactus DB website](https://galactusdb.com) · [Source](https://github.com/galactusdb/galactus-db-rust-driver) · [Type mapping](docs/BLUEPRINT.md) · [Spatial types](docs/SPATIAL.md)

Native Bolt 4.4 driver for Galactus DB. This experimental 0.1 implementation
has zero third-party runtime package dependencies. Source is available here;
no npm, NuGet, PyPI, Maven Central, or crates.io release is implied.

## How To

### 1. Get the driver

```sh
git clone --branch main https://github.com/galactusdb/galactus-db-rust-driver.git
cd galactus-db-rust-driver
```

Rust 1.70+; no dependencies. Add this to your application Cargo.toml:

```toml
[dependencies]
galactus = { package = "galactus-db-rust-driver", git = "https://github.com/galactusdb/galactus-db-rust-driver", branch = "main" }
```

### 2. Configure your connection

Start or obtain a Galactus DB instance; visit [galactusdb.com](https://galactusdb.com)
for database information. Use its Bolt address (locally, `bolt://127.0.0.1:7687`),
username, and configured password. The example reads `GDB_PASSWORD` from your
environment; it is an application variable, not a command to change the server password.

```sh
# Bash / zsh
export GDB_PASSWORD='your-database-password'
```

```powershell
# PowerShell
$env:GDB_PASSWORD = 'your-database-password'
```

### 3. Execute a parameterised query

```rust
use galactus::{Driver, Map};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut driver = Driver::connect("bolt://127.0.0.1:7687", "gdb",
        &std::env::var("GDB_PASSWORD")?, "neo4j", Duration::from_secs(30))?;
    let result = driver.execute_query("RETURN $name AS name",
        Map::from([("name".into(), "Ada".into())]))?;
    println!("{}", result.records[0]["name"].as_str().unwrap());
    Ok(()) // Drop closes the connection.
}
```

`begin(read_only)`, `commit()`, `rollback()`, and `close()` manage lifecycle.
Mutable access enforces one operation at a time. Open another driver for
concurrent work. TCP only: use a TLS tunnel if required; `bolt+s://` is rejected.

`Value` has native primitive/container variants and named graph, temporal,
point and Spatial variants. `From` / `TryFrom` handle native conversion with
range checks. `Value::try_from(SystemTime)` and `SystemTime::try_from(value)`
preserve instants where the native range allows. No chrono/serde dependency
is required. Match a returned `Value::Spatial(shape)` to inspect its metadata
and WKB, then bind `shape.into()` to a query.

See [mapping](docs/BLUEPRINT.md), [spatial examples](docs/SPATIAL.md), and [scope](docs/OVERVIEW.md#scope-of-01).

### 4. Run the tests

From this repository's root:

```sh
cargo test --offline
```

See [test instructions](tests/README.md) for prerequisites and opt-in live tests.
Connection failures discard the connection; writes are never automatically retried.
Results are eager and each driver owns one connection; see the documented scope.

Learn more at [Galactus DB website](https://galactusdb.com).
