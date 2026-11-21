//! The mapper's shared test (docs/MAPPER.md), and its checks that need no database.
use galactus::mapper::{from_node, valid_name, Error, Mapped, Mapper};
use galactus::{Map, Node, Value};

galactus::node! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Person: "Person" {
        key id: i64,
        name: String,
        email as "email_address": Option<String>,
        age: Option<i64>,
    }
}
galactus::node! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Company: "Company" { key name: String }
}
galactus::relationship! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct WorksAt: "WORKS_AT" { since: i64, role: String }
}
galactus::node! {
    #[derive(Debug, Clone, PartialEq)]
    struct Note: "Note" { text: String, tags: Vec<String> }
}

fn person(id: i64, name: &str) -> Person {
    Person { id, name: name.into(), email: None, age: None }
}
fn is_mapping<T: std::fmt::Debug>(result: Result<T, Error>) -> bool {
    matches!(result, Err(Error::Mapping(_)))
}

#[test]
fn properties_and_conversion() {
    let ann = Person { email: Some("ann@example.com".into()), ..person(1, "Ann") };
    assert_eq!(
        ann.properties(),
        Map::from([
            ("id".into(), Value::Int(1)),
            ("name".into(), "Ann".into()),
            ("email_address".into(), "ann@example.com".into()),
            ("age".into(), Value::Null),
        ])
    );
    assert_eq!(Person::KEY, Some("id"));
    assert_eq!(Person::key_property(), Some("id"));
    assert_eq!(WorksAt::KEY, None);
    assert_eq!(Person::FIELDS[2], ("email", "email_address"));
    let node = |properties: Map| Node { id: 7, labels: vec!["Person".into()], properties, element_id: "7".into() };
    let stored = node(Map::from([
        ("id".into(), Value::Int(1)),
        ("name".into(), "Ann".into()),
        ("email_address".into(), "a@b".into()),
        ("extra".into(), Value::Bool(true)),
    ]));
    assert_eq!(from_node::<Person>(&stored).unwrap(), Person { email: Some("a@b".into()), ..person(1, "Ann") });
    // A node that lacks properties leaves the fields empty.
    assert_eq!(from_node::<Person>(&node(Map::from([("id".into(), Value::Int(2))]))).unwrap(), person(2, ""));
    // A value of the wrong kind names the type and the field.
    let wrong = from_node::<Person>(&node(Map::from([("id".into(), "one".into())])));
    assert!(matches!(wrong, Err(Error::Mapping(message)) if message.starts_with("Person.id:")));
    let note = Note { text: "hi".into(), tags: vec!["a".into(), "b".into()] };
    assert_eq!(Note::from_properties(&note.properties()).unwrap(), note);
    assert!(valid_name("Person") && valid_name("email address"));
    assert!(!valid_name("") && !valid_name("a`b"));
}

#[test]
fn the_shared_story() {
    let Ok(uri) = std::env::var("GDB_TEST_URI") else {
        eprintln!("mapper live test skipped: set GDB_TEST_URI");
        return;
    };
    let password = std::env::var("GDB_TEST_PASSWORD").unwrap();
    #[allow(unused_mut)]
    let mut options = galactus::ConnectOptions::default();
    if let Ok(ca) = std::env::var("GDB_TEST_CA_FILE") {
        options = options.trusted_certificates(ca);
    }
    let mut driver = galactus::Driver::connect_with_options(
        &uri,
        "gdb",
        &password,
        "",
        std::time::Duration::from_secs(30),
        &options,
    )
    .unwrap();
    driver
        .execute_query("MATCH (n) WHERE n:Person OR n:Company OR n:Note DETACH DELETE n", Map::new())
        .unwrap();
    let mut mapper = Mapper::new(&mut driver);
    let ids = |people: Vec<Person>| people.into_iter().map(|p| p.id).collect::<Vec<_>>();

    // 2. create, find, and the stored property name
    let mut ann = Person { email: Some("ann@example.com".into()), ..person(1, "Ann") };
    assert_eq!(mapper.create(&ann).unwrap(), ann);
    assert_eq!(mapper.find::<Person>(1).unwrap(), Some(ann.clone()));
    let raw = mapper
        .driver()
        .execute_query("MATCH (p:Person {id: 1}) RETURN p.email_address AS e, p.email AS none", Map::new())
        .unwrap();
    assert_eq!(raw.records[0]["e"], Value::from("ann@example.com"));
    assert_eq!(raw.records[0]["none"], Value::Null);
    // 3. save changes, and creates
    ann.age = Some(41);
    assert_eq!(mapper.save(&ann).unwrap().age, Some(41));
    assert_eq!(mapper.save(&person(2, "Bob")).unwrap(), person(2, "Bob"));
    // 4. a missing key
    assert_eq!(mapper.find::<Person>(99).unwrap(), None);
    // 5. find all
    mapper.save(&Person { age: Some(41), ..person(3, "Bob") }).unwrap();
    assert_eq!(ids(mapper.find_all(&[], None).unwrap()), [1, 2, 3]);
    assert_eq!(ids(mapper.find_all(&[("name", "Bob".into())], None).unwrap()), [2, 3]);
    assert_eq!(ids(mapper.find_all(&[("name", "Bob".into()), ("age", 41i64.into())], None).unwrap()), [3]);
    assert_eq!(ids(mapper.find_all(&[], Some(2)).unwrap()), [1, 2]);
    assert!(is_mapping(mapper.find_all::<Person>(&[("height", Value::Int(1))], None)));
    // 6. relate and related
    let acme = mapper.save(&Company { name: "Acme".into() }).unwrap();
    let engineer = WorksAt { since: 2020, role: "engineer".into() };
    let lead = WorksAt { since: 2021, role: "lead".into() };
    assert_eq!(mapper.relate(&ann, &engineer, &acme).unwrap(), engineer);
    assert_eq!(mapper.relate(&ann, &lead, &acme).unwrap(), lead);
    assert_eq!(mapper.related::<WorksAt, Company, _>(&ann).unwrap(), vec![(lead.clone(), acme.clone())]);
    assert!(is_mapping(mapper.relate(&person(77, "Nobody"), &lead, &acme)));
    // 7. a hand-written statement
    let found: Vec<Person> = mapper
        .query(
            "MATCH (p:Person) WHERE p.age >= $age RETURN p ORDER BY p.id",
            Map::from([("age".into(), Value::Int(41))]),
        )
        .unwrap();
    assert_eq!(ids(found), [1, 3]);
    let made: Vec<Company> = mapper
        .query("RETURN {name: $n} AS c", Map::from([("n".into(), "Made up".into())]))
        .unwrap();
    assert_eq!(made, vec![Company { name: "Made up".into() }]);
    // 8. delete
    assert!(mapper.delete::<Person>(3).unwrap());
    assert!(!mapper.delete::<Person>(3).unwrap());
    // 9. no key
    let note = Note { text: "hello".into(), tags: vec!["first".into()] };
    assert_eq!(mapper.create(&note).unwrap(), note);
    assert!(is_mapping(mapper.save(&note)));
    assert!(is_mapping(mapper.find::<Note>("x")));
    assert!(is_mapping(mapper.find::<Person>(Value::Null)));
    // the operations join an open transaction
    mapper.driver().begin(false).unwrap();
    mapper.save(&person(50, "Temporary")).unwrap();
    mapper.driver().rollback().unwrap();
    assert_eq!(mapper.find::<Person>(50).unwrap(), None);
}
