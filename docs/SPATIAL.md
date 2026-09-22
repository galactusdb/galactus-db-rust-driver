# Spatial mapping

Every driver maps legacy Bolt points to `Point2D` / `Point3D`, including SRID
7203 (Cartesian), 9157 (Cartesian XYZ), 4326 (WGS84), and 4979 (WGS84 XYZ).

Every geometry/geography value maps to `Spatial(domain, srid, layout, model,
wkb)`. The binary WKB is retained exactly. This supports all server shape
families, polygon holes, multipart geometries, nested GeometryCollections, typed
empties, XY, and XYZ. Geography uses model **`greatCircle`**; geometry uses
**`planar`**. SRID does not select the domain or perform reprojection.

## Wire contract

```text
{
  "$gdbType": "spatial",
  "version": 1,
  "domain": "geometry",             // or geography
  "srid": 7203,
  "layout": "XY",                   // or XYZ
  "model": "planar",                // geography: greatCircle
  "wkb": <native bytes>
}
```

This is an ordinary PackStream map. All drivers recursively hydrate recognized
version-1 envelopes, including envelopes in node/relationship properties, lists,
and maps. Unknown envelope versions stay ordinary maps so metadata is not lost.
Known versions with invalid metadata fail. `from_map` / `fromMap` / `FromMap`
also accepts explicit `wkbHex` instead of `wkb`; encoding always uses bytes.

The `Spatial` value is an interchange type, not a geometry engine. The server
validates WKB structure, coordinate counts, dimensions, SRIDs, topology, and
geographic bounds. The driver checks envelope metadata and preserves payloads;
it does not claim that arbitrary user-constructed WKB is valid geometry.

## Construct, store, and round-trip

Python example; the same queries and parameter names work in all seven drivers:

```python
shape = driver.execute_query(
    "RETURN spatial.fromWKT($wkt, {domain:$domain}) AS shape",
    {"wkt": "POLYGON((0 0,4 0,4 4,0 4,0 0))", "domain": "geometry"},
).records[0]["shape"]

driver.execute_query(
    "CREATE (:Region {name:$name, shape:spatial.fromMap($shape)})",
    {"name": "Square", "shape": shape},
)

restored = driver.execute_query(
    "MATCH (r:Region {name:$name}) RETURN r.shape AS shape",
    {"name": "Square"},
).records[0]["shape"]
assert restored.wkb == shape.wkb
```

Binding `Spatial` sends its lossless envelope. Use `spatial.fromMap($shape)` to
promote the parameter to a database geometry/geography value. `RETURN $shape`
echoes an envelope; it does not change the server's MAP type. This distinction
is part of the current server contract and avoids accidental map promotion.

To import external WKB without knowing its envelope metadata, use
`RETURN spatial.fromWKB($bytes, {domain:$domain}) AS shape`. To import WKT or
GeoJSON, use the corresponding server constructor. All drivers bind the text,
bytes, or nested native map directly. Geography GeoJSON import requires
`{domain:'geography', edges:'greatCircle'}`. Export with `spatial.asWKT`,
`spatial.asEWKB`, `spatial.asGeoJSON`, or the other database functions.

WKB without the envelope loses domain and SRID. GeoJSON conversion can lose
typed empties and has geography approximation rules; it is never used as an
implicit transport representation. Curves, surfaces, solids, M and ZM are
currently unsupported by the database, and are not advertised by the drivers.

See the authoritative [database spatial documentation](https://galactusdb.com)
and [spatial test fixtures](../tests/fixtures/spatial.tsv).
