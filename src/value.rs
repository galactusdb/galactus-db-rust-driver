use crate::{wire::Ps, Error, Map, Result};
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
#[derive(Debug, Clone, PartialEq)]
pub struct ZonedDateTime {
    pub seconds: i64,
    pub nanoseconds: i64,
    pub zone_id: String,
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
    pub(crate) fn to_wire(&self, depth: usize) -> Result<Ps> {
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
                    .map(|x| x.to_wire(depth + 1))
                    .collect::<Result<_>>()?,
            ),
            Self::Map(v) => Ps::Map(
                v.iter()
                    .map(|(k, x)| Ok((k.clone(), x.to_wire(depth + 1)?)))
                    .collect::<Result<_>>()?,
            ),
            Self::Structure(tag, f) => {
                if f.len() > 15 {
                    return Err(Error::Protocol("invalid structure".into()));
                }
                Ps::Struct(
                    *tag,
                    f.iter()
                        .map(|x| x.to_wire(depth + 1))
                        .collect::<Result<_>>()?,
                )
            }
            Self::Spatial(s) => Value::Map(s.to_map()).to_wire(depth + 1)?,
            Self::Node(v) => Value::Structure(
                78,
                vec![
                    Value::from(v.id.clone()),
                    Value::List(v.labels.clone()),
                    Value::Map(v.properties.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::Relationship(v) => Value::Structure(
                82,
                vec![
                    Value::from(v.id.clone()),
                    Value::from(v.start_id.clone()),
                    Value::from(v.end_id.clone()),
                    Value::from(v.kind.clone()),
                    Value::Map(v.properties.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::UnboundRelationship(v) => Value::Structure(
                114,
                vec![
                    Value::from(v.id.clone()),
                    Value::from(v.kind.clone()),
                    Value::Map(v.properties.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::Path(v) => Value::Structure(
                80,
                vec![
                    Value::List(v.nodes.clone()),
                    Value::List(v.relationships.clone()),
                    Value::List(v.sequence.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::Date(v) => {
                Value::Structure(68, vec![Value::from(v.days.clone())]).to_wire(depth + 1)?
            }
            Self::LocalTime(v) => Value::Structure(116, vec![Value::from(v.nanoseconds.clone())])
                .to_wire(depth + 1)?,
            Self::Time(v) => Value::Structure(
                84,
                vec![
                    Value::from(v.nanoseconds.clone()),
                    Value::from(v.offset_seconds.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::LocalDateTime(v) => Value::Structure(
                100,
                vec![
                    Value::from(v.seconds.clone()),
                    Value::from(v.nanoseconds.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::DateTime(v) => Value::Structure(
                70,
                vec![
                    Value::from(v.seconds.clone()),
                    Value::from(v.nanoseconds.clone()),
                    Value::from(v.offset_seconds.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::ZonedDateTime(v) => Value::Structure(
                102,
                vec![
                    Value::from(v.seconds.clone()),
                    Value::from(v.nanoseconds.clone()),
                    Value::from(v.zone_id.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::Duration(v) => Value::Structure(
                69,
                vec![
                    Value::from(v.months.clone()),
                    Value::from(v.days.clone()),
                    Value::from(v.seconds.clone()),
                    Value::from(v.nanoseconds.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::Point2D(v) => Value::Structure(
                88,
                vec![
                    Value::from(v.srid.clone()),
                    Value::from(v.x.clone()),
                    Value::from(v.y.clone()),
                ],
            )
            .to_wire(depth + 1)?,
            Self::Point3D(v) => Value::Structure(
                89,
                vec![
                    Value::from(v.srid.clone()),
                    Value::from(v.x.clone()),
                    Value::from(v.y.clone()),
                    Value::from(v.z.clone()),
                ],
            )
            .to_wire(depth + 1)?,
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
                    78 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Node(Node {
                            id: <i64>::try_from(it.next().unwrap())?,
                            labels: <Vec<Value>>::try_from(it.next().unwrap())?,
                            properties: <Map>::try_from(it.next().unwrap())?,
                        })
                    }
                    82 => {
                        if values.len() != 5 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::Relationship(Relationship {
                            id: <i64>::try_from(it.next().unwrap())?,
                            start_id: <i64>::try_from(it.next().unwrap())?,
                            end_id: <i64>::try_from(it.next().unwrap())?,
                            kind: <String>::try_from(it.next().unwrap())?,
                            properties: <Map>::try_from(it.next().unwrap())?,
                        })
                    }
                    114 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::UnboundRelationship(UnboundRelationship {
                            id: <i64>::try_from(it.next().unwrap())?,
                            kind: <String>::try_from(it.next().unwrap())?,
                            properties: <Map>::try_from(it.next().unwrap())?,
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
                    70 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::DateTime(DateTime {
                            seconds: <i64>::try_from(it.next().unwrap())?,
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                            offset_seconds: <i64>::try_from(it.next().unwrap())?,
                        })
                    }
                    102 => {
                        if values.len() != 3 {
                            return Err(Error::Protocol("invalid structure field count".into()));
                        }
                        let mut it = values.into_iter();
                        Self::ZonedDateTime(ZonedDateTime {
                            seconds: <i64>::try_from(it.next().unwrap())?,
                            nanoseconds: <i64>::try_from(it.next().unwrap())?,
                            zone_id: <String>::try_from(it.next().unwrap())?,
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
