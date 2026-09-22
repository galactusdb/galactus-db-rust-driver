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
        assert_eq!(encode(&decode(&bytes).unwrap()).unwrap(), bytes, "{name}");
    }
    for h in include_str!("fixtures/malformed.tsv").lines() {
        assert!(decode(&unhex(h)).is_err(), "{h}")
    }
}
#[test]
fn live() {
    let Ok(uri) = std::env::var("GDB_TEST_URI") else {
        eprintln!("live skipped: set GDB_TEST_URI");
        return;
    };
    let password = std::env::var("GDB_TEST_PASSWORD").unwrap();
    let mut d = Driver::connect(
        &uri,
        "gdb",
        &password,
        "neo4j",
        std::time::Duration::from_secs(30),
    )
    .unwrap();
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
        assert_eq!(n.properties["n"], 2i64.into())
    } else {
        panic!("expected Node")
    }
    assert!(matches!(
        d.execute_query("INVALID QUERY", Map::new()),
        Err(Error::Database { .. })
    ));
    assert!(d.execute_query("RETURN 1", Map::new()).is_err());
    assert!(matches!(
        Driver::connect(
            &uri,
            "gdb",
            "wrong-password",
            "neo4j",
            std::time::Duration::from_secs(30)
        ),
        Err(Error::Database { .. })
    ));
}
