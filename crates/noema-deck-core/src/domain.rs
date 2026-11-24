//! Stars: the topics on the map.

use std::{fmt, num::ParseIntError, str::FromStr};

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

/// A star's identity. It is never reused, even after the star is deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StarId(i64);

impl StarId {
    pub(crate) const fn new(id: i64) -> Self {
        Self(id)
    }
}

impl fmt::Display for StarId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for StarId {
    type Err = ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse().map(Self)
    }
}

impl ToSql for StarId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for StarId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i64::column_result(value).map(Self)
    }
}

/// A topic on the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Star {
    pub id: StarId,
    pub name: String,
    pub x: f64,
    pub y: f64,
}
