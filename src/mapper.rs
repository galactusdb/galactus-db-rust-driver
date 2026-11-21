//! Object mapping: nodes and relationships as the application's own structs,
//! and the Cypher for everyday operations. Standard library only;
//! `docs/MAPPER.md` is the contract every Galactus mapper follows.
//!
//! ```no_run
//! use galactus::mapper::Mapper;
//!
//! galactus::node! {
//!     #[derive(Debug, Clone, PartialEq)]
//!     pub struct Person: "Person" {
//!         key id: i64,
//!         name: String,
//!         email as "email_address": Option<String>,
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut driver = galactus::Driver::connect(
//!     "bolt://localhost:7687", "gdb", "password", "", std::time::Duration::from_secs(30))?;
//! let mut people = Mapper::new(&mut driver);
//! people.save(&Person { id: 1, name: "Ann".into(), email: None })?;
//! let ann: Option<Person> = people.find(1)?;
//! # Ok(()) }
//! ```
//!
//! [`node!`](crate::node) and [`relationship!`](crate::relationship) declare a
//! struct and implement [`Mapped`] for it. A field is `name: Type`, stored
//! under its own name; `name as "property": Type`, stored under another; and
//! in a node, one field may begin `key`. A label, type or property name that
//! is empty or holds a backtick does not compile.
use crate::{Driver, Map, Value};

/// A type that cannot be mapped or a value that does not fit it, or a failure
/// of the driver underneath.
#[derive(Debug)]
pub enum Error {
    Mapping(String),
    Driver(crate::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mapping(message) => write!(f, "{message}"),
            Self::Driver(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<crate::Error> for Error {
    fn from(error: crate::Error) -> Self {
        Self::Driver(error)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

fn mapping<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::Mapping(message.into()))
}

/// Whether a label, relationship type or property name can be written into
/// Cypher inside backticks: it is not empty and holds no backtick.
pub const fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            return false;
        }
        at += 1;
    }
    !bytes.is_empty()
}

/// A type a mapped field can have.
pub trait Field: Sized {
    /// The field from a database value that is not null.
    fn from_value(value: Value) -> std::result::Result<Self, String>;
    /// The field when the property is absent or null.
    fn empty() -> Self;
    fn to_value(&self) -> Value;
}
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Int(_) => "an integer",
        Value::Float(_) => "a float",
        Value::String(_) => "a string",
        Value::Bytes(_) => "bytes",
        Value::List(_) => "a list",
        Value::Map(_) => "a map",
        _ => "a structured value",
    }
}
macro_rules! whole_number {
    ($($t:ty),*) => {$(
        impl Field for $t {
            fn from_value(value: Value) -> std::result::Result<Self, String> {
                match value {
                    Value::Int(n) => <$t>::try_from(n).map_err(|_| format!("{n} does not fit {}", stringify!($t))),
                    other => Err(format!("expected an integer, got {}", kind(&other))),
                }
            }
            fn empty() -> Self {
                0
            }
            fn to_value(&self) -> Value {
                Value::Int(i64::from(*self))
            }
        }
    )*};
}
whole_number!(i64, i32, i16, i8, u32, u16);
impl Field for f64 {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        match value {
            Value::Float(x) => Ok(x),
            Value::Int(n) => Ok(n as f64),
            other => Err(format!("expected a number, got {}", kind(&other))),
        }
    }
    fn empty() -> Self {
        0.0
    }
    fn to_value(&self) -> Value {
        Value::Float(*self)
    }
}
impl Field for bool {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        match value {
            Value::Bool(b) => Ok(b),
            other => Err(format!("expected a boolean, got {}", kind(&other))),
        }
    }
    fn empty() -> Self {
        false
    }
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
}
impl Field for String {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        match value {
            Value::String(s) => Ok(s),
            other => Err(format!("expected a string, got {}", kind(&other))),
        }
    }
    fn empty() -> Self {
        String::new()
    }
    fn to_value(&self) -> Value {
        Value::String(self.clone())
    }
}
impl Field for Vec<u8> {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        match value {
            Value::Bytes(b) => Ok(b),
            other => Err(format!("expected bytes, got {}", kind(&other))),
        }
    }
    fn empty() -> Self {
        Vec::new()
    }
    fn to_value(&self) -> Value {
        Value::Bytes(self.clone())
    }
}
/// Any value, as the driver gives it.
impl Field for Value {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        Ok(value)
    }
    fn empty() -> Self {
        Value::Null
    }
    fn to_value(&self) -> Value {
        self.clone()
    }
}
impl<T: Field> Field for Option<T> {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        T::from_value(value).map(Some)
    }
    fn empty() -> Self {
        None
    }
    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Null, Field::to_value)
    }
}
impl<T: Field> Field for Vec<T> {
    fn from_value(value: Value) -> std::result::Result<Self, String> {
        match value {
            Value::List(items) => items
                .into_iter()
                .map(|item| match item {
                    Value::Null => Ok(T::empty()),
                    item => T::from_value(item),
                })
                .collect(),
            other => Err(format!("expected a list, got {}", kind(&other))),
        }
    }
    fn empty() -> Self {
        Vec::new()
    }
    fn to_value(&self) -> Value {
        Value::List(self.iter().map(Field::to_value).collect())
    }
}

/// What the mapper knows of a declared type. Implemented by
/// [`node!`](crate::node) and [`relationship!`](crate::relationship).
pub trait Mapped: Sized {
    /// The label or relationship type.
    const NAME: &'static str;
    /// Each field and the property it is stored as.
    const FIELDS: &'static [(&'static str, &'static str)];
    /// The field that is the key, when the type has one.
    const KEY: Option<&'static str>;
    /// Every declared property, by its stored name.
    fn properties(&self) -> Map;
    fn from_properties(properties: &Map) -> Result<Self>;

    /// The property the key is stored as.
    fn key_property() -> Option<&'static str> {
        let key = Self::KEY?;
        Self::FIELDS.iter().find(|(field, _)| *field == key).map(|(_, stored)| *stored)
    }
}
/// A [`Mapped`] type declared with [`node!`](crate::node).
pub trait NodeType: Mapped {}
/// A [`Mapped`] type declared with [`relationship!`](crate::relationship).
pub trait RelationshipType: Mapped {}

/// An object of a node type from a node.
pub fn from_node<T: NodeType>(node: &crate::Node) -> Result<T> {
    T::from_properties(&node.properties)
}
/// An object of a relationship type from a relationship.
pub fn from_relationship<T: RelationshipType>(relationship: &crate::Relationship) -> Result<T> {
    T::from_properties(&relationship.properties)
}

fn quote(name: &str) -> String {
    format!("`{name}`")
}
fn from_value<T: Mapped>(value: Option<Value>) -> Result<T> {
    match value {
        Some(Value::Node(node)) => T::from_properties(&node.properties),
        Some(Value::Relationship(relationship)) => T::from_properties(&relationship.properties),
        Some(Value::UnboundRelationship(relationship)) => T::from_properties(&relationship.properties),
        Some(Value::Map(properties)) => T::from_properties(&properties),
        Some(other) => mapping(format!(
            "{}: expected a node, a relationship or a map, got {}",
            T::NAME,
            kind(&other)
        )),
        None => mapping(format!("{}: the row has no such column", T::NAME)),
    }
}
/// The key's property and its value in `object`, which must have both.
fn key_of<T: NodeType>(object: &T) -> Result<(&'static str, Value)> {
    let property = key_property::<T>()?;
    let value = object.properties().remove(property).unwrap_or(Value::Null);
    Ok((property, checked_key::<T>(value)?))
}
fn key_property<T: NodeType>() -> Result<&'static str> {
    match T::key_property() {
        Some(property) => Ok(property),
        None => mapping(format!("{} has no key", T::NAME)),
    }
}
fn checked_key<T: NodeType>(value: Value) -> Result<Value> {
    match value {
        Value::Null => mapping(format!("{}: the key has no value", T::NAME)),
        value => Ok(value),
    }
}
fn match_key<T: NodeType>(variable: &str, parameter: &str) -> Result<String> {
    Ok(format!(
        "({variable}:{} {{{}: ${parameter}}})",
        quote(T::NAME),
        quote(key_property::<T>()?)
    ))
}

/// Runs the everyday operations on a driver, each as one statement.
pub struct Mapper<'d> {
    driver: &'d mut Driver,
}
impl<'d> Mapper<'d> {
    pub fn new(driver: &'d mut Driver) -> Self {
        Self { driver }
    }
    /// The driver underneath, for a statement or a transaction of your own.
    pub fn driver(&mut self) -> &mut Driver {
        self.driver
    }

    fn rows(&mut self, query: &str, parameters: Map) -> Result<Vec<Map>> {
        Ok(self.driver.execute_query(query, parameters)?.records)
    }

    /// Make a new node of the object. Returns it as stored.
    pub fn create<T: NodeType>(&mut self, object: &T) -> Result<T> {
        let query = format!("CREATE (n:{}) SET n = $properties RETURN n", quote(T::NAME));
        let mut rows = self.rows(&query, Map::from([("properties".into(), Value::Map(object.properties()))]))?;
        from_value(rows.pop().and_then(|mut row| row.remove("n")))
    }

    /// Create the node with the object's key, or change it. Returns it as stored.
    pub fn save<T: NodeType>(&mut self, object: &T) -> Result<T> {
        let (_, key) = key_of(object)?;
        let query = format!("MERGE {} SET n += $properties RETURN n", match_key::<T>("n", "key")?);
        let parameters = Map::from([("key".into(), key), ("properties".into(), Value::Map(object.properties()))]);
        let mut rows = self.rows(&query, parameters)?;
        from_value(rows.pop().and_then(|mut row| row.remove("n")))
    }

    /// The node with the key, when there is one.
    pub fn find<T: NodeType>(&mut self, key: impl Into<Value>) -> Result<Option<T>> {
        let query = format!("MATCH {} RETURN n LIMIT 1", match_key::<T>("n", "key")?);
        let mut rows = self.rows(&query, Map::from([("key".into(), checked_key::<T>(key.into())?)]))?;
        match rows.pop() {
            Some(mut row) => from_value(row.remove("n")).map(Some),
            None => Ok(None),
        }
    }

    /// Every node of the type whose fields equal those in `filter`, which
    /// names fields of the struct.
    pub fn find_all<T: NodeType>(&mut self, filter: &[(&str, Value)], limit: Option<u32>) -> Result<Vec<T>> {
        let mut parameters = Map::new();
        let mut conditions = Vec::new();
        for (index, (field, value)) in filter.iter().enumerate() {
            let Some((_, stored)) = T::FIELDS.iter().find(|(name, _)| name == field) else {
                return mapping(format!("{} has no field `{field}` to filter by", T::NAME));
            };
            parameters.insert(format!("p{index}"), value.clone());
            conditions.push(format!("n.{} = $p{index}", quote(stored)));
        }
        let mut query = format!("MATCH (n:{})", quote(T::NAME));
        if !conditions.is_empty() {
            query += &format!(" WHERE {}", conditions.join(" AND "));
        }
        query += " RETURN n";
        if let Some(key) = T::key_property() {
            query += &format!(" ORDER BY n.{}", quote(key));
        }
        if let Some(limit) = limit {
            parameters.insert("limit".into(), Value::Int(i64::from(limit)));
            query += " LIMIT $limit";
        }
        self.rows(&query, parameters)?
            .into_iter()
            .map(|mut row| from_value(row.remove("n")))
            .collect()
    }

    /// Remove the node with the key and its relationships. Whether there was one.
    pub fn delete<T: NodeType>(&mut self, key: impl Into<Value>) -> Result<bool> {
        let query = format!("MATCH {} DETACH DELETE n RETURN count(*) AS deleted", match_key::<T>("n", "key")?);
        let rows = self.rows(&query, Map::from([("key".into(), checked_key::<T>(key.into())?)]))?;
        Ok(matches!(rows.first().and_then(|row| row.get("deleted")), Some(Value::Int(n)) if *n > 0))
    }

    /// Join two stored nodes with a relationship, or change the one between
    /// them. Returns it as stored.
    pub fn relate<A: NodeType, R: RelationshipType, B: NodeType>(&mut self, from: &A, relationship: &R, to: &B) -> Result<R> {
        let ((_, a), (_, b)) = (key_of(from)?, key_of(to)?);
        let query = format!(
            "MATCH {}, {} MERGE (a)-[r:{}]->(b) SET r += $properties RETURN r",
            match_key::<A>("a", "a")?,
            match_key::<B>("b", "b")?,
            quote(R::NAME)
        );
        let parameters = Map::from([
            ("a".into(), a),
            ("b".into(), b),
            ("properties".into(), Value::Map(relationship.properties())),
        ]);
        match self.rows(&query, parameters)?.pop() {
            Some(mut row) => from_value(row.remove("r")),
            None => mapping(format!("{} or {} was not found to relate", A::NAME, B::NAME)),
        }
    }

    /// The relationships of type `R` from a node to nodes of type `B`.
    pub fn related<R: RelationshipType, B: NodeType, A: NodeType>(&mut self, from: &A) -> Result<Vec<(R, B)>> {
        let (_, a) = key_of(from)?;
        let mut query = format!(
            "MATCH {}-[r:{}]->(b:{}) RETURN r, b",
            match_key::<A>("a", "a")?,
            quote(R::NAME),
            quote(B::NAME)
        );
        if let Some(key) = B::key_property() {
            query += &format!(" ORDER BY b.{}", quote(key));
        }
        self.rows(&query, Map::from([("a".into(), a)]))?
            .into_iter()
            .map(|mut row| Ok((from_value(row.remove("r"))?, from_value(row.remove("b"))?)))
            .collect()
    }

    /// Run a statement and map the first column of each row: a node, a
    /// relationship, or a map of properties.
    pub fn query<T: Mapped>(&mut self, cypher: &str, parameters: Map) -> Result<Vec<T>> {
        let result = self.driver.execute_query(cypher, parameters)?;
        let Some(column) = result.keys.first() else {
            return Ok(Vec::new());
        };
        result
            .records
            .into_iter()
            .map(|mut row| from_value(row.remove(column)))
            .collect()
    }
}

/// Declare a node type: a struct, its label, and its fields.
///
/// ```
/// galactus::node! {
///     #[derive(Debug, PartialEq)]
///     pub struct Person: "Person" {
///         key id: i64,
///         name: String,
///         email as "email_address": Option<String>,
///     }
/// }
/// ```
#[macro_export]
macro_rules! node {
    ($(#[$meta:meta])* $vis:vis struct $name:ident : $label:literal { $($body:tt)* }) => {
        $crate::__galactus_mapped!(@fields [$(#[$meta])*] [$vis] $name $label NodeType [None] [] $($body)*);
    };
}

/// Declare a relationship type: a struct, its type name, and its fields.
///
/// ```
/// galactus::relationship! {
///     #[derive(Debug, PartialEq)]
///     pub struct WorksAt: "WORKS_AT" { since: i64, role: String }
/// }
/// ```
#[macro_export]
macro_rules! relationship {
    ($(#[$meta:meta])* $vis:vis struct $name:ident : $kind:literal { $($body:tt)* }) => {
        $crate::__galactus_mapped!(@fields [$(#[$meta])*] [$vis] $name $kind RelationshipType [Some(())] [] $($body)*);
    };
}

// Reads the fields one at a time into `[field, property, type]` triples, then
// writes the struct and its `Mapped` implementation. The key slot starts as
// `[None]` in a node and takes one field; a relationship's starts full, so a
// `key` there does not match.
#[doc(hidden)]
#[macro_export]
macro_rules! __galactus_mapped {
    (@fields $meta:tt $vis:tt $name:ident $label:literal $marker:ident [None] [$($done:tt)*]
        key $field:ident as $stored:literal : $t:ty $(, $($rest:tt)*)?) => {
        $crate::__galactus_mapped!(@fields $meta $vis $name $label $marker [Some(stringify!($field))]
            [$($done)* [$field, $stored, $t]] $($($rest)*)?);
    };
    (@fields $meta:tt $vis:tt $name:ident $label:literal $marker:ident [None] [$($done:tt)*]
        key $field:ident : $t:ty $(, $($rest:tt)*)?) => {
        $crate::__galactus_mapped!(@fields $meta $vis $name $label $marker [Some(stringify!($field))]
            [$($done)* [$field, stringify!($field), $t]] $($($rest)*)?);
    };
    (@fields $meta:tt $vis:tt $name:ident $label:literal $marker:ident $key:tt [$($done:tt)*]
        $field:ident as $stored:literal : $t:ty $(, $($rest:tt)*)?) => {
        $crate::__galactus_mapped!(@fields $meta $vis $name $label $marker $key
            [$($done)* [$field, $stored, $t]] $($($rest)*)?);
    };
    (@fields $meta:tt $vis:tt $name:ident $label:literal $marker:ident $key:tt [$($done:tt)*]
        $field:ident : $t:ty $(, $($rest:tt)*)?) => {
        $crate::__galactus_mapped!(@fields $meta $vis $name $label $marker $key
            [$($done)* [$field, stringify!($field), $t]] $($($rest)*)?);
    };
    (@fields [$(#[$meta:meta])*] [$vis:vis] $name:ident $label:literal $marker:ident [$key:expr]
        [$([$field:ident, $stored:expr, $t:ty])*]) => {
        $(#[$meta])*
        $vis struct $name {
            $(pub $field: $t,)*
        }
        const _: () = {
            assert!($crate::mapper::valid_name($label), "a label or relationship type cannot be empty or hold a backtick");
            $(assert!($crate::mapper::valid_name($stored), "a property name cannot be empty or hold a backtick");)*
        };
        impl $crate::mapper::Mapped for $name {
            const NAME: &'static str = $label;
            const FIELDS: &'static [(&'static str, &'static str)] = &[$((stringify!($field), $stored)),*];
            const KEY: Option<&'static str> = $crate::__galactus_mapped!(@key $marker $key);
            fn properties(&self) -> $crate::Map {
                let mut properties = $crate::Map::new();
                $(properties.insert($stored.to_string(), $crate::mapper::Field::to_value(&self.$field));)*
                properties
            }
            fn from_properties(properties: &$crate::Map) -> $crate::mapper::Result<Self> {
                Ok(Self {
                    $($field: match properties.get($stored) {
                        None | Some($crate::Value::Null) => $crate::mapper::Field::empty(),
                        Some(value) => $crate::mapper::Field::from_value(value.clone()).map_err(|problem| {
                            $crate::mapper::Error::Mapping(format!(
                                "{}.{}: {problem}", stringify!($name), stringify!($field)))
                        })?,
                    },)*
                })
            }
        }
        impl $crate::mapper::$marker for $name {}
    };
    (@key NodeType $key:expr) => { $key };
    (@key RelationshipType $key:expr) => { None };
}
