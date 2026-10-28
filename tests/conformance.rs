use galactus::*;
#[test]
fn native_clock_roundtrip() {
    let time = std::time::UNIX_EPOCH - std::time::Duration::from_nanos(123456789);
    let value = Value::try_from(time).unwrap();
    assert_eq!(
        std::time::SystemTime::try_from(decode(&encode(&value).unwrap()).unwrap()).unwrap(),
        time
    );
    assert!(Value::try_from(u64::MAX).is_err());
}
fn unhex(h: &str) -> Vec<u8> {
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn golden() {
    for line in include_str!("fixtures/values.tsv").lines() {
        let (name, h) = line.split_once('\t').unwrap();
        let bytes = unhex(h);
        let value = decode(&bytes).unwrap();
        assert_eq!(encode(&value).unwrap(), bytes, "{name}");
        assert_eq!(
            encode_for(&value, Protocol::Bolt44).unwrap(),
            bytes,
            "{name}"
        );
    }
    for h in include_str!("fixtures/malformed.tsv").lines() {
        assert!(decode(&unhex(h)).is_err(), "{h}")
    }
}
#[test]
fn golden_bolt5() {
    for line in include_str!("fixtures/bolt5.tsv").lines() {
        let (name, h) = line.split_once('\t').unwrap();
        let bytes = unhex(h);
        let value = decode(&bytes).unwrap();
        assert_eq!(
            encode_for(&value, Protocol::Bolt5).unwrap(),
            bytes,
            "{name}"
        );
    }
}
#[test]
fn datetime_pairs() {
    for line in include_str!("fixtures/datetime_pairs.tsv").lines() {
        let (bolt44, bolt5) = line.split_once('\t').unwrap();
        let (bolt44, bolt5) = (unhex(bolt44), unhex(bolt5));
        let value = decode(&bolt44).unwrap();
        assert_eq!(decode(&bolt5).unwrap(), value, "{line}");
        assert!(matches!(value, Value::DateTime(_)));
        assert_eq!(
            encode_for(&value, Protocol::Bolt44).unwrap(),
            bolt44,
            "{line}"
        );
        assert_eq!(
            encode_for(&value, Protocol::Bolt5).unwrap(),
            bolt5,
            "{line}"
        );
    }
}
#[test]
fn element_ids_and_zone_basis() {
    // A Bolt 4.4 node gets its element id from the integer id ...
    let node = decode(&unhex("b34e019186506572736f6ea1846e616d6583416461")).unwrap();
    let Value::Node(n) = &node else {
        panic!("expected Node")
    };
    assert_eq!(n.element_id, "1");
    // ... and is written with it in Bolt 5 form.
    assert_eq!(
        encode_for(&node, Protocol::Bolt5).unwrap(),
        unhex("b44e019186506572736f6ea1846e616d65834164618131")
    );
    let rel = decode(&unhex("b5520201038552454c4154a0")).unwrap();
    let Value::Relationship(r) = rel else {
        panic!("expected Relationship")
    };
    assert_eq!(
        (
            r.element_id.as_str(),
            r.start_element_id.as_str(),
            r.end_element_id.as_str()
        ),
        ("2", "1", "3")
    );
    // Same numbers, different basis: not equal, and each keeps its own tag.
    let local = ZonedDateTime::local(3600, 0, "Europe/London");
    let utc = ZonedDateTime::utc(3600, 0, "Europe/London");
    assert_ne!(local, utc);
    for p in [Protocol::Bolt44, Protocol::Bolt5] {
        assert_eq!(encode_for(&local.clone().into(), p).unwrap()[1], 0x66);
        assert_eq!(encode_for(&utc.clone().into(), p).unwrap()[1], 0x69);
    }
}
#[test]
fn error_code_constants() {
    assert_eq!(
        codes::CONSTRAINT_VALIDATION_FAILED,
        "Galactus.ClientError.Schema.ConstraintValidationFailed"
    );
    assert_eq!(
        codes::LICENSE_RESTRICTED,
        "Galactus.ClientError.License.Restricted"
    );
    assert_eq!(codes::SYNTAX_ERROR, "Neo.ClientError.Statement.SyntaxError");
}

fn connect(uri: &str, password: &str, ca: Option<&str>) -> Result<Driver> {
    let mut options = ConnectOptions::new();
    if let Some(ca) = ca {
        options = options.trusted_certificates(ca);
    }
    Driver::connect_with_options(
        uri,
        "gdb",
        password,
        "",
        std::time::Duration::from_secs(30),
        &options,
    )
}
fn one(d: &mut Driver, query: &str, params: Map) -> Map {
    d.execute_query(query, params).unwrap().records.remove(0)
}

#[test]
fn live() {
    let Ok(uri) = std::env::var("GDB_TEST_URI") else {
        eprintln!("live skipped: set GDB_TEST_URI");
        return;
    };
    let password = std::env::var("GDB_TEST_PASSWORD").unwrap();
    let ca = std::env::var("GDB_TEST_CA_FILE").ok();
    let mut d = connect(&uri, &password, ca.as_deref()).unwrap();
    if let Ok(expect) = std::env::var("GDB_TEST_EXPECT_BOLT") {
        assert_eq!(d.protocol_version(), expect);
    }
    let bolt5 = d.protocol() == Protocol::Bolt5;
    assert!(!d.server_agent().is_empty());
    let values = Map::from([
        ("n".into(), i64::MAX.into()),
        ("b".into(), Value::Bytes(vec![0, 255])),
        ("s".into(), "x".repeat(70000).into()),
        (
            "t".into(),
            DateTime {
                seconds: -315615477,
                nanoseconds: 456789123,
                offset_seconds: 19800,
            }
            .into(),
        ),
    ]);
    assert_eq!(
        d.execute_query(
            "RETURN $v AS v",
            Map::from([("v".into(), Value::Map(values.clone()))])
        )
        .unwrap()
        .records[0]["v"],
        Value::Map(values)
    );
    // Offset and named-zone datetimes. The server answers named zones in its
    // own form for the protocol: UTC-based on Bolt 5, local-based on 4.4.
    let offset: Value = DateTime {
        seconds: 1782907200,
        nanoseconds: 123456789,
        offset_seconds: 3600,
    }
    .into();
    let local = ZonedDateTime::local(1782907200, 5, "Europe/London");
    let utc = ZonedDateTime::utc(1782907200 - 3600, 5, "Europe/London");
    let native: Value = if bolt5 { utc.clone() } else { local.clone() }.into();
    for v in [&offset, &native] {
        let row = one(
            &mut d,
            "RETURN $v AS v",
            Map::from([("v".into(), v.clone())]),
        );
        assert_eq!(&row["v"], v);
    }
    // The other basis is accepted too and comes back as the same instant.
    let other: Value = if bolt5 { local } else { utc }.into();
    let row = one(&mut d, "RETURN $v AS v", Map::from([("v".into(), other)]));
    assert_eq!(row["v"], native);

    for line in include_str!("fixtures/spatial.tsv").lines() {
        let (domain, wkt) = line.split_once('\t').unwrap();
        let s = d
            .execute_query(
                "RETURN spatial.fromWKT($wkt,{domain:$domain}) AS shape",
                Map::from([("domain".into(), domain.into()), ("wkt".into(), wkt.into())]),
            )
            .unwrap()
            .records
            .remove(0)
            .remove("shape")
            .unwrap();
        assert!(matches!(s, Value::Spatial(_)));
        let nested = Value::List(vec![Value::Map(Map::from([("shape".into(), s.clone())]))]);
        let row = d
            .execute_query(
                "RETURN spatial.fromMap($s) AS shape, $nested AS nested",
                Map::from([("s".into(), s.clone()), ("nested".into(), nested.clone())]),
            )
            .unwrap()
            .records
            .remove(0);
        assert_eq!(row["shape"], s, "{line}");
        assert_eq!(row["nested"], nested);
    }
    d.begin(false).unwrap();
    d.execute_query("CREATE (:DriverRust {n:1})", Map::new())
        .unwrap();
    d.rollback().unwrap();
    assert_eq!(
        d.execute_query("MATCH (n:DriverRust) RETURN count(n) AS n", Map::new())
            .unwrap()
            .records[0]["n"],
        0i64.into()
    );
    d.begin(false).unwrap();
    d.execute_query("CREATE (:DriverRust {n:2})", Map::new())
        .unwrap();
    d.commit().unwrap();
    let row = d
        .execute_query("MATCH (n:DriverRust) RETURN n", Map::new())
        .unwrap()
        .records
        .remove(0);
    if let Value::Node(n) = &row["n"] {
        assert_eq!(n.properties["n"], 2i64.into());
        assert_eq!(n.element_id, n.id.to_string());
    } else {
        panic!("expected Node")
    }
    // Element ids are the decimal id text on every protocol.
    let row = one(
        &mut d,
        "CREATE (a:DriverRustGraph)-[r:DRIVER_RUST_LINK]->(b:DriverRustGraph) \
         RETURN a, r, b, elementId(a) AS ea",
        Map::new(),
    );
    let (Value::Node(a), Value::Relationship(r), Value::Node(b)) =
        (&row["a"], &row["r"], &row["b"])
    else {
        panic!("expected node, relationship, node: {row:?}")
    };
    assert_eq!(a.element_id, a.id.to_string());
    assert_eq!(b.element_id, b.id.to_string());
    assert_eq!(row["ea"], Value::from(a.element_id.clone()));
    assert_eq!(r.element_id, r.id.to_string());
    assert_eq!(r.start_element_id, a.element_id);
    assert_eq!(r.end_element_id, b.element_id);
    assert_eq!(r.start_element_id, r.start_id.to_string());
    assert_eq!(r.end_element_id, r.end_id.to_string());

    let (limited, truncated) = d
        .execute_query_limited("UNWIND range(1, 5) AS n RETURN n", Map::new(), 2)
        .unwrap();
    assert_eq!(limited.records.len(), 2);
    assert!(truncated);
    let (whole, truncated) = d
        .execute_query_limited("UNWIND range(1, 2) AS n RETURN n", Map::new(), 5)
        .unwrap();
    assert_eq!(whole.records.len(), 2);
    assert!(!truncated);
    // The discarded remainder leaves the connection ready for the next query.
    assert_eq!(
        d.execute_query("RETURN 1 AS one", Map::new())
            .unwrap()
            .records[0]["one"],
        1i64.into()
    );
    // Decimals arrive as strings holding their exact digits.
    let row = one(
        &mut d,
        "RETURN decimal($d) AS d",
        Map::from([("d".into(), "12.30".into())]),
    );
    assert_eq!(row["d"], Value::from("12.30"));

    let mut timed = connect(&uri, &password, ca.as_deref()).unwrap();
    timed.set_transaction_timeout(Some(std::time::Duration::from_millis(1)));
    let err = timed
        .execute_query(
            "UNWIND range(1, 20000000) AS x RETURN count(x) AS c",
            Map::new(),
        )
        .unwrap_err();
    assert_eq!(err.code(), Some(codes::TRANSACTION_TIMED_OUT), "{err}");
    timed.set_transaction_timeout(None);
    assert_eq!(
        one(&mut timed, "RETURN 1 AS one", Map::new())["one"],
        1i64.into()
    );

    // A database error resets the connection; the same driver keeps working.
    let err = d.execute_query("INVALID QUERY", Map::new()).unwrap_err();
    assert!(matches!(err, Error::Database { .. }), "{err}");
    assert!(err.is_client_error(), "{err}");
    assert_eq!(err.classification(), Some("ClientError"));
    assert!(err.category().is_some() && err.title().is_some());
    assert!(!err.is_retryable());
    assert_eq!(
        one(&mut d, "RETURN 1 AS one", Map::new())["one"],
        1i64.into()
    );

    // A failure inside a transaction closes it, rolling back its writes.
    d.begin(false).unwrap();
    d.execute_query("CREATE (:DriverRustTx {n:1})", Map::new())
        .unwrap();
    assert!(matches!(
        d.execute_query("INVALID QUERY", Map::new()),
        Err(Error::Database { .. })
    ));
    assert!(matches!(d.commit(), Err(Error::Protocol(_))));
    assert_eq!(
        one(
            &mut d,
            "MATCH (n:DriverRustTx) RETURN count(n) AS n",
            Map::new()
        )["n"],
        0i64.into()
    );
    d.begin(false).unwrap();
    d.rollback().unwrap();

    // Unique constraint violations carry the published code.
    if let Err(e) = d.execute_query(
        "CREATE CONSTRAINT FOR (n:DriverRustUnique) REQUIRE n.key IS UNIQUE",
        Map::new(),
    ) {
        // Only acceptable when an equivalent constraint already exists.
        assert!(matches!(e, Error::Database { .. }), "{e}");
    }
    d.execute_query("CREATE (:DriverRustUnique {key:'rust-unique'})", Map::new())
        .unwrap();
    let err = d
        .execute_query("CREATE (:DriverRustUnique {key:'rust-unique'})", Map::new())
        .unwrap_err();
    assert_eq!(
        err.code(),
        Some(codes::CONSTRAINT_VALIDATION_FAILED),
        "{err}"
    );
    assert_eq!(err.title(), Some("ConstraintValidationFailed"));
    assert!(err.is_client_error());
    assert_eq!(
        one(
            &mut d,
            "MATCH (n:DriverRustUnique {key:'rust-unique'}) RETURN count(n) AS n",
            Map::new()
        )["n"],
        1i64.into()
    );

    #[cfg(feature = "tls")]
    if let Some(ca) = ca.as_deref() {
        // The test CA is not in the platform store, so verification fails.
        match connect(&uri, &password, None) {
            Err(Error::Tls(m)) => assert!(m.to_lowercase().contains("certificate"), "{m}"),
            Err(e) => panic!("expected a TLS error, got {e}"),
            Ok(_) => panic!("connected without trusting the test CA"),
        }
        let plain = uri.replacen("bolt+s://", "bolt://", 1);
        assert!(matches!(
            connect(&plain, &password, Some(ca)),
            Err(Error::Config(_))
        ));
    }

    let err = match connect(&uri, "wrong-password", ca.as_deref()) {
        Err(e) => e,
        Ok(_) => panic!("wrong password accepted"),
    };
    assert!(matches!(err, Error::Database { .. }), "{err}");
    assert!(err.is_client_error(), "{err}");
}
