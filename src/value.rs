use crate::{wire::Ps, Error, Map, Protocol, Result};
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(Map),
    Structure(u8, Vec<Value>),
    Spatial(Spatial),
    Node(Node),
    Relationship(Relationship),
    UnboundRelationship(UnboundRelationship),
    Path(Path),
    Date(Date),
    LocalTime(LocalTime),
    Time(Time),
    LocalDateTime(LocalDateTime),
    DateTime(DateTime),
    ZonedDateTime(ZonedDateTime),
    Duration(Duration),
    Point2D(Point2D),
    Point3D(Point3D),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: i64,
    pub labels: Vec<Value>,
    pub properties: Map,
    /// Bolt 5 element id. On Bolt 4.4 the driver fills it with the decimal
    /// text of `id`, which is what Galactus's `elementId()` returns.
    pub element_id: String,
}
impl From<Node> for Value {
    fn from(v: Node) -> Self {
        Self::Node(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    pub id: i64,
    pub start_id: i64,
    pub end_id: i64,
    pub kind: String,
    pub properties: Map,
    pub element_id: String,
    pub start_element_id: String,
    pub end_element_id: String,
}
impl From<Relationship> for Value {
    fn from(v: Relationship) -> Self {
        Self::Relationship(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct UnboundRelationship {
    pub id: i64,
    pub kind: String,
    pub properties: Map,
    pub element_id: String,
}
impl From<UnboundRelationship> for Value {
    fn from(v: UnboundRelationship) -> Self {
        Self::UnboundRelationship(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub nodes: Vec<Value>,
    pub relationships: Vec<Value>,
    pub sequence: Vec<Value>,
}
impl From<Path> for Value {
    fn from(v: Path) -> Self {
        Self::Path(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Date {
    pub days: i64,
}
impl From<Date> for Value {
    fn from(v: Date) -> Self {
        Self::Date(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct LocalTime {
    pub nanoseconds: i64,
}
impl From<LocalTime> for Value {
    fn from(v: LocalTime) -> Self {
        Self::LocalTime(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Time {
    pub nanoseconds: i64,
    pub offset_seconds: i64,
}
impl From<Time> for Value {
    fn from(v: Time) -> Self {
        Self::Time(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct LocalDateTime {
    pub seconds: i64,
    pub nanoseconds: i64,
}
impl From<LocalDateTime> for Value {
    fn from(v: LocalDateTime) -> Self {
        Self::LocalDateTime(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DateTime {
    pub seconds: i64,
    pub nanoseconds: i64,
    pub offset_seconds: i64,
}
impl From<DateTime> for Value {
    fn from(v: DateTime) -> Self {
        Self::DateTime(v)
    }
}
/// Which epoch a [`ZonedDateTime`]'s `seconds` count from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoneBasis {
    /// Local wall-clock epoch seconds in the zone (Bolt tag `f`).
    #[default]
    Local,
    /// UTC epoch seconds (Bolt tag `i`).
    Utc,
}
/// A named-zone datetime. Converting between local and UTC seconds needs a
/// time zone database, so the value keeps the basis it was given and the
/// encoder writes that basis unchanged (`f` for local, `i` for UTC) on every
/// protocol version. Two values are equal only when basis, seconds,
/// nanoseconds and zone all match.
#[derive(Debug, Clone, PartialEq)]
pub struct ZonedDateTime {
    pub seconds: i64,
    pub nanoseconds: i64,
    pub zone_id: String,
    pub basis: ZoneBasis,
}
impl ZonedDateTime {
    /// `seconds` are local epoch seconds in `zone_id`.
    pub fn local(seconds: i64, nanoseconds: i64, zone_id: impl Into<String>) -> Self {
        Self {
            seconds,
            nanoseconds,
            zone_id: zone_id.into(),
            basis: ZoneBasis::Local,
        }
    }
    /// `seconds` are UTC epoch seconds.
    pub fn utc(seconds: i64, nanoseconds: i64, zone_id: impl Into<String>) -> Self {
        Self {
            seconds,
            nanoseconds,
            zone_id: zone_id.into(),
            basis: ZoneBasis::Utc,
        }
    }
}
impl From<ZonedDateTime> for Value {
    fn from(v: ZonedDateTime) -> Self {
        Self::ZonedDateTime(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Duration {
    pub months: i64,
    pub days: i64,
    pub seconds: i64,
    pub nanoseconds: i64,
}
impl From<Duration> for Value {
    fn from(v: Duration) -> Self {
        Self::Duration(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Point2D {
    pub srid: i64,
    pub x: f64,
    pub y: f64,
}
impl From<Point2D> for Value {
    fn from(v: Point2D) -> Self {
        Self::Point2D(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Point3D {
    pub srid: i64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl From<Point3D> for Value {
    fn from(v: Point3D) -> Self {
        Self::Point3D(v)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Spatial {
    pub domain: String,
    pub srid: i64,
    pub layout: String,
    pub model: String,
    pub wkb: Vec<u8>,
}
impl Spatial {
    pub fn to_map(&self) -> Map {
        Map::from([
            ("$gdbType".into(), "spatial".into()),
            ("version".into(), 1i64.into()),
            ("domain".into(), self.domain.clone().into()),
            ("srid".into(), self.srid.into()),
            ("layout".into(), self.layout.clone().into()),
            ("model".into(), self.model.clone().into()),
            ("wkb".into(), Value::Bytes(self.wkb.clone())),
        ])
    }
    pub fn from_map(mut m: Map) -> Result<Self> {
        if m.len() != 7
            || m.get("$gdbType") != Some(&Value::from("spatial"))
            || m.get("version") != Some(&Value::Int(1))
        {
            return Err(Error::Protocol("invalid spatial envelope".into()));
        }
        let domain = String::try_from(m.remove("domain").unwrap_or(Value::Null))?;
        let srid = i64::try_from(m.remove("srid").unwrap_or(Value::Null))?;
        let layout = String::try_from(m.remove("layout").unwrap_or(Value::Null))?;
        let model = String::try_from(m.remove("model").unwrap_or(Value::Null))?;
        if !matches!(domain.as_str(), "geometry" | "geography")
            || !matches!(layout.as_str(), "XY" | "XYZ")
            || model
                != if domain == "geometry" {
                    "planar"
                } else {
                    "greatCircle"
                }
        {
            return Err(Error::Protocol("unsupported spatial metadata".into()));
        }
        let wkb = match m.remove("wkb") {
            Some(Value::Bytes(b)) => b,
            None => {
                let hex = String::try_from(m.remove("wkbHex").unwrap_or(Value::Null))?;
                if hex.len() % 2 != 0 || !hex.is_ascii() {
                    return Err(Error::Protocol("invalid WKB hex".into()));
                }
                (0..hex.len())
                    .step_by(2)
                    .map(|i| {
                        u8::from_str_radix(&hex[i..i + 2], 16)
                            .map_err(|_| Error::Protocol("invalid WKB hex".into()))
                    })
                    .collect::<Result<Vec<u8>>>()?
            }
            _ => return Err(Error::Protocol("invalid WKB".into())),
        };
        Ok(Self {
            domain,
            srid,
            layout,
            model,
            wkb,
        })
    }
}
impl From<Spatial> for Value {
    fn from(v: Spatial) -> Self {
        Self::Spatial(v)
    }
}
impl Value {
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        if let Self::Int(i) = self {
            Some(*i)
        } else {
            None
        }
    }
    pub(crate) fn to_wire(&self, depth: usize, protocol: Protocol) -> Result<Ps> {
        if depth >= 64 {
            return Err(Error::Protocol("nesting exceeds 64 levels".into()));
        }
        Ok(match self {
            Self::Null => Ps::Null,
            Self::Bool(v) => Ps::Bool(*v),
            Self::Int(v) => Ps::Int(*v),
            Self::Float(v) => Ps::Float(*v),
            Self::String(v) => Ps::String(v.clone()),
            Self::Bytes(v) => Ps::Bytes(v.clone()),
            Self::List(v) => Ps::List(
                v.iter()
                    .map(|x| x.to_wire(depth + 1, protocol))
                    .collect::<Result<_>>()?,
            ),
            Self::Map(v) => Ps::Map(
                v.iter()
                    .map(|(k, x)| Ok((k.clone(), x.to_wire(depth + 1, protocol)?)))
                    .collect::<Result<_>>()?,
            ),
            Self::Structure(tag, f) => {
                if f.len() > 15 {
                    return Err(Error::Protocol("invalid structure".into()));
                }
                Ps::Struct(
                    *tag,
                    f.iter()
                        .map(|x| x.to_wire(depth + 1, protocol))
                        .collect::<Result<_>>()?,
                )
            }
            Self::Spatial(s) => Value::Map(s.to_map()).to_wire(depth + 1, protocol)?,
            Self::Node(v) => {
                let mut f = vec![
                    Value::from(v.id),
                    Value::List(v.labels.clone()),
                    Value::Map(v.properties.clone()),
                ];
                if protocol == Protocol::Bolt5 {
                    f.push(v.element_id.clone().into());
                }
                Value::Structure(78, f).to_wire(depth + 1, protocol)?
            }
            Self::Relationship(v) => {
                let mut f = vec![
                    Value::from(v.id),
                    Value::from(v.start_id),
                    Value::from(v.end_id),
                    Value::from(v.kind.clone()),
                    Value::Map(v.properties.clone()),
                ];
                if protocol == Protocol::Bolt5 {
                    f.push(v.element_id.clone().into());
                    f.push(v.start_element_id.clone().into());
                    f.push(v.end_element_id.clone().into());
                }
                Value::Structure(82, f).to_wire(depth + 1, protocol)?
            }
            Self::UnboundRelationship(v) => {
                let mut f = vec![
                    Value::from(v.id),
                    Value::from(v.kind.clone()),
                    Value::Map(v.properties.clone()),
                ];
                if protocol == Protocol::Bolt5 {
                    f.push(v.element_id.clone().into());
                }
                Value::Structure(114, f).to_wire(depth + 1, protocol)?
            }
            Self::Path(v) => Value::Structure(
                80,
                vec![
                    Value::List(v.nodes.clone()),
                    Value::List(v.relationships.clone()),
                    Value::List(v.sequence.clone()),
                ],
            )
            .to_wire(depth + 1, protocol)?,
            Self::Date(v) => {
                Value::Structure(68, vec![Value::from(v.days)]).to_wire(depth + 1, protocol)?
            }
            Self::LocalTime(v) => Value::Structure(116, vec![Value::from(v.nanoseconds)])
                .to_wire(depth + 1, protocol)?,
            Self::Time(v) => Value::Structure(
                84,
                vec![Value::from(v.nanoseconds), Value::from(v.offset_seconds)],
            )
            .to_wire(depth + 1, protocol)?,
            Self::LocalDateTime(v) => Value::Structure(
                100,
                vec![Value::from(v.seconds), Value::from(v.nanoseconds)],
            )
            .to_wire(depth + 1, protocol)?,
            // `DateTime::seconds` are local epoch seconds (tag F). Bolt 5
            // sends UTC seconds (tag I); the conversion is exact.
            Self::DateTime(v) => {
                let (tag, seconds) = if protocol == Protocol::Bolt5 {
                    let utc = v.seconds.checked_sub(v.offset_seconds).ok_or_else(|| {
                        Error::Protocol("datetime outside signed 64-bit range".into())
                    })?;
                    (0x49, utc)
                } else {
                    (0x46, v.seconds)
                };
                Value::Structure(
                    tag,
                    vec![
                        Value::from(seconds),
                        Value::from(v.nanoseconds),
                        Value::from(v.offset_seconds),
                    ],
                )
                .to_wire(depth + 1, protocol)?
            }
            Self::ZonedDateTime(v) => Value::Structure(
                match v.basis {
                    ZoneBasis::Local => 0x66,
                    ZoneBasis::Utc => 0x69,
                },
                vec![
                    Value::from(v.seconds),
                    Value::from(v.nanoseconds),
                    Value::from(v.zone_id.clone()),
                ],
            )
            .to_wire(depth + 1, protocol)?,
            Self::Duration(v) => Value::Structure(
                69,
                vec![
                    Value::from(v.months),
                    Value::from(v.days),
                    Value::from(v.seconds),
                    Value::from(v.nanoseconds),
                ],
            )
            .to_wire(depth + 1, protocol)?,
            Self::Point2D(v) => Value::Structure(
                88,
                vec![Value::from(v.srid), Value::from(v.x), Value::from(v.y)],
            )
            .to_wire(depth + 1, protocol)?,
            Self::Point3D(v) => Value::Structure(
                89,
                vec![
                    Value::from(v.srid),
                    Value::from(v.x),
                    Value::from(v.y),
                    Value::from(v.z),
                ],
            )
            .to_wire(depth + 1, protocol)?,
        })
    }
    pub(crate) fn from_wire(v: Ps) -> Result<Self> {
        Ok(match v {
            Ps::Null => Self::Null,
            Ps::Bool(v) => Self::Bool(v),
            Ps::Int(v) => Self::Int(v),
            Ps::Float(v) => Self::Float(v),
            Ps::String(v) => Self::String(v),
            Ps::Bytes(v) => Self::Bytes(v),
            Ps::List(v) => Self::List(v.into_iter().map(Self::from_wire).collect::<Result<_>>()?),
            Ps::Map(v) => {
                let mut m = Map::new();
                for (k, v) in v {
                    if m.insert(k, Self::from_wire(v)?).is_some() {
                        return Err(Error::Protocol("duplicate map key".into()));
                    }
                }
                if m.get("$gdbType") == Some(&Value::from("spatial"))
                    && m.get("version") == Some(&Value::Int(1))
                {
                    Self::Spatial(Spatial::from_map(m)?)
                } else {
                    Self::Map(m)
                }
            }
            Ps::Struct(tag, f) => {
                let values = f
                    .into_iter()
                    .map(Self::from_wire)
                    .collect::<Result<Vec<_>>>()?;
                match tag {
                    // Bolt 4.4 sends 3/5/3 fields; Bolt 5 appends element ids.
                    78 => {
                        if values.len() != 3 && values.len() != 4 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        let id = <i64>::try_from(it.next().unwrap())?;
                        let labels = <Vec<Value>>::try_from(it.next().unwrap())?;
                        let properties = <Map>::try_from(it.next().unwrap())?;
                        Self::Node(Node {
                            id,
                            labels,
                            properties,
                            element_id: element_id(it.next(), id)?,
                        })
                    }
                    82 => {
                        if values.len() != 5 && values.len() != 8 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        let id = <i64>::try_from(it.next().unwrap())?;
                        let start_id = <i64>::try_from(it.next().unwrap())?;
                        let end_id = <i64>::try_from(it.next().unwrap())?;
                        let kind = <String>::try_from(it.next().unwrap())?;
                        let properties = <Map>::try_from(it.next().unwrap())?;
                        Self::Relationship(Relationship {
                            id,
                            start_id,
                            end_id,
                            kind,
                            properties,
                            element_id: element_id(it.next(), id)?,
                            start_element_id: element_id(it.next(), start_id)?,
                            end_element_id: element_id(it.next(), end_id)?,
                        })
                    }
                    114 => {
                        if values.len() != 3 && values.len() != 4 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        let id = <i64>::try_from(it.next().unwrap())?;
                        let kind = <String>::try_from(it.next().unwrap())?;
                        let properties = <Map>::try_from(it.next().unwrap())?;
                        Self::UnboundRelationship(UnboundRelationship {
                            id,
                            kind,
                            properties,
                            element_id: element_id(it.next(), id)?,
                        })
                    }
                    80 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Path(Path {
                            nodes: <Vec<Value>>::try_from(it.next().unwrap())?,
                            relationships: <Vec<Value>>::try_from(it.next().unwrap())?,
                            sequence: <Vec<Value>>::try_from(it.next().unwrap())?,
                        })
                    }
                    68 => {
                        if values.len() != 1 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Date(Date {
                            days: <i64>::try_from(it.next().unwrap())?,
                        })
                    }
                    116 => {
                        if values.len() != 1 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::LocalTime(LocalTime {
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                        })
                    }
                    84 => {
                        if values.len() != 2 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Time(Time {
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                            offset_seconds: <i64>::try_from(it.next().unwrap())?,
                        })
                    }
                    100 => {
                        if values.len() != 2 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::LocalDateTime(LocalDateTime {
                            seconds: <i64>::try_from(it.next().unwrap())?,
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                        })
                    }
                    // F (local seconds) and I (UTC seconds), on any version.
                    0x46 | 0x49 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        let seconds = <i64>::try_from(it.next().unwrap())?;
                        let nanoseconds = <i64>::try_from(it.next().unwrap())?;
                        let offset_seconds = <i64>::try_from(it.next().unwrap())?;
                        let seconds = if tag == 0x49 {
                            seconds.checked_add(offset_seconds).ok_or_else(|| {
                                Error::Protocol("datetime outside signed 64-bit range".into())
                            })?
                        } else {
                            seconds
                        };
                        Self::DateTime(DateTime {
                            seconds,
                            nanoseconds,
                            offset_seconds,
                        })
                    }
                    // f (local seconds) and i (UTC seconds); the basis is kept.
                    0x66 | 0x69 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::ZonedDateTime(ZonedDateTime {
                            seconds: <i64>::try_from(it.next().unwrap())?,
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                            zone_id: <String>::try_from(it.next().unwrap())?,
                            basis: if tag == 0x69 {
                                ZoneBasis::Utc
                            } else {
                                ZoneBasis::Local
                            },
                        })
                    }
                    69 => {
                        if values.len() != 4 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Duration(Duration {
                            months: <i64>::try_from(it.next().unwrap())?,
                            days: <i64>::try_from(it.next().unwrap())?,
                            seconds: <i64>::try_from(it.next().unwrap())?,
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                        })
                    }
                    88 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Point2D(Point2D {
                            srid: <i64>::try_from(it.next().unwrap())?,
                            x: <f64>::try_from(it.next().unwrap())?,
                            y: <f64>::try_from(it.next().unwrap())?,
                        })
                    }
                    89 => {
                        if values.len() != 4 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Point3D(Point3D {
                            srid: <i64>::try_from(it.next().unwrap())?,
                            x: <f64>::try_from(it.next().unwrap())?,
                            y: <f64>::try_from(it.next().unwrap())?,
                            z: <f64>::try_from(it.next().unwrap())?,
                        })
                    }
                    _ => Self::Structure(tag, values),
                }
            }
        })
    }
}
/// A Bolt 5 element id field, or the decimal id text when it is absent (4.4).
fn element_id(field: Option<Value>, id: i64) -> Result<String> {
    match field {
        Some(v) => String::try_from(v),
        None => Ok(id.to_string()),
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl TryFrom<Value> for bool {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::Bool(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected bool".into()))
        }
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl TryFrom<Value> for i64 {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::Int(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected i64".into()))
        }
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}
impl TryFrom<Value> for f64 {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::Float(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected f64".into()))
        }
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}
impl TryFrom<Value> for String {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::String(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected String".into()))
        }
    }
}
impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}
impl TryFrom<Value> for Vec<u8> {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::Bytes(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected Vec<u8>".into()))
        }
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Self::List(v)
    }
}
impl TryFrom<Value> for Vec<Value> {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::List(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected Vec<Value>".into()))
        }
    }
}
impl From<Map> for Value {
    fn from(v: Map) -> Self {
        Self::Map(v)
    }
}
impl TryFrom<Value> for Map {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        if let Value::Map(v) = v {
            Ok(v)
        } else {
            Err(Error::Protocol("expected Map".into()))
        }
    }
}
impl From<i8> for Value {
    fn from(v: i8) -> Self {
        Self::Int(v as i64)
    }
}
impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Self::Int(v as i64)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Int(v as i64)
    }
}
impl From<u8> for Value {
    fn from(v: u8) -> Self {
        Self::Int(v as i64)
    }
}
impl From<u16> for Value {
    fn from(v: u16) -> Self {
        Self::Int(v as i64)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Self::Int(v as i64)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.into())
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map(Into::into).unwrap_or(Self::Null)
    }
}
impl TryFrom<u64> for Value {
    type Error = Error;
    fn try_from(v: u64) -> Result<Self> {
        Ok(Self::Int(i64::try_from(v).map_err(|_| {
            Error::Protocol("integer outside signed 64-bit range".into())
        })?))
    }
}
impl TryFrom<std::time::SystemTime> for Value {
    type Error = Error;
    fn try_from(v: std::time::SystemTime) -> Result<Self> {
        let (seconds, nanoseconds) = match v.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => (
                i64::try_from(d.as_secs())
                    .map_err(|_| Error::Protocol("datetime outside signed 64-bit range".into()))?,
                d.subsec_nanos() as i64,
            ),
            Err(e) => {
                let d = e.duration();
                let seconds = i64::try_from(d.as_secs())
                    .map_err(|_| Error::Protocol("datetime outside signed 64-bit range".into()))?;
                if d.subsec_nanos() == 0 {
                    (-seconds, 0)
                } else {
                    (-seconds - 1, 1_000_000_000 - d.subsec_nanos() as i64)
                }
            }
        };
        Ok(DateTime {
            seconds,
            nanoseconds,
            offset_seconds: 0,
        }
        .into())
    }
}
impl TryFrom<std::time::Duration> for Value {
    type Error = Error;
    fn try_from(v: std::time::Duration) -> Result<Self> {
        Ok(Duration {
            months: 0,
            days: 0,
            seconds: i64::try_from(v.as_secs())
                .map_err(|_| Error::Protocol("duration outside signed 64-bit range".into()))?,
            nanoseconds: v.subsec_nanos() as i64,
        }
        .into())
    }
}
impl TryFrom<Value> for std::time::SystemTime {
    type Error = Error;
    fn try_from(v: Value) -> Result<Self> {
        let d = if let Value::DateTime(d) = v {
            d
        } else {
            return Err(Error::Protocol("expected DateTime".into()));
        };
        if !(0..1_000_000_000).contains(&d.nanoseconds) {
            return Err(Error::Protocol("invalid nanoseconds".into()));
        }
        let secs = d
            .seconds
            .checked_sub(d.offset_seconds)
            .ok_or_else(|| Error::Protocol("datetime overflow".into()))?;
        let epoch = std::time::UNIX_EPOCH;
        let base = if secs >= 0 {
            epoch.checked_add(std::time::Duration::from_secs(secs as u64))
        } else {
            epoch.checked_sub(std::time::Duration::from_secs(secs.unsigned_abs()))
        };
        base.and_then(|b| b.checked_add(std::time::Duration::from_nanos(d.nanoseconds as u64)))
            .ok_or_else(|| Error::Protocol("datetime outside native range".into()))
    }
}
