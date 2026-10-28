# Changelog

## 0.2.0 (28/10/2026)

### Added

- Bolt 5.0 to 5.4, with a Bolt 4.4 fallback. On 5.1 and newer the driver sends
  `bolt_agent` in HELLO and authenticates with a separate LOGON.
- `protocol_version()`, `server_agent()`, `connection_id()` and
  `last_bookmark()` on `Driver`.
- Recovery after database errors: the driver sends RESET, closes any open
  transaction and stays ready for the next query.
- Error classification on `Error`: `code()`, `classification()`, `category()`,
  `title()`, `is_client_error()`, `is_transient()`, `is_database_error()` and
  `is_retryable()`, plus the `codes` module of published error codes.
- Verified `bolt+s://` behind the `tls` cargo feature (rustls with the ring
  provider and the platform verifier), and `ConnectOptions::trusted_certificates`
  for a PEM file that replaces the platform store. `Driver::connect_with_options`
  takes the options.
- `element_id` on `Node`, `Relationship` and `UnboundRelationship`, and
  `start_element_id` / `end_element_id` on `Relationship`. On Bolt 4.4 they hold
  the decimal text of the integer ids.
- Bolt 5 datetimes: `I` and `i` are decoded on any version, offset datetimes
  are sent as `I` on 5.x and `F` on 4.4, and `encode_for(value, Protocol)`
  exposes the protocol-aware encoder.
- `ZonedDateTime::basis` (`ZoneBasis::Local` or `ZoneBasis::Utc`) with the
  `ZonedDateTime::local` and `ZonedDateTime::utc` constructors. Values are sent
  in their own basis (`f` or `i`) on every protocol.
- `Error::Config` for unusable URIs or options and `Error::Tls` for failed TLS
  handshakes.

### Changed

- `user_agent` is now `galactus-db-rust-driver/0.2.0`.
- A database error no longer discards the connection. I/O, TLS and protocol
  failures still do.
- A parameter that cannot be sent raises before anything is written and leaves
  the connection usable.
- Unsupported URI schemes, URI query options, `bolt+ssc://` and routing URIs
  now give `Error::Config` instead of `Error::Protocol`. Without the `tls`
  feature, `bolt+s://` gives `Error::Config` naming the feature.
- Struct literals for `Node`, `Relationship`, `UnboundRelationship` and
  `ZonedDateTime` need the new fields; prefer `ZonedDateTime::local` or
  `ZonedDateTime::utc`. Exhaustive matches on `Error` need the two new variants.

### Notes

- The default build still has no dependencies. `DECIMAL` values arrive as
  strings holding their exact digits; send strings and use `decimal($param)`.
