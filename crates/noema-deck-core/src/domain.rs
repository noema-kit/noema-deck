//! Domains: the subjects sessions are grouped under.

use std::{fmt, num::ParseIntError, str::FromStr};

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

/// A domain's identity. It is never reused, even after the domain is deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DomainId(i64);

impl DomainId {
    pub(crate) const fn new(id: i64) -> Self {
        Self(id)
    }
}

impl fmt::Display for DomainId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for DomainId {
    type Err = ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse().map(Self)
    }
}

impl ToSql for DomainId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for DomainId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i64::column_result(value).map(Self)
    }
}

/// A subject that sessions are grouped under, drawn as a star on the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Domain {
    pub id: DomainId,
    pub name: String,
    pub x: f64,
    pub y: f64,
}
