//! Backend-neutral query description shared by the Turso, TiDB and Firestore crates.
//!
//! The wire names (`equalTo`, `whereIn`, …) match `masamune_model_turso` / `masamune_model_tidb`
//! so the same [`Query`] can be sent to the Masamune Cloudflare workers as-is. [`Query::apply`]
//! evaluates the same query against cached rows, which is what makes offline reads possible.

use std::cmp::Ordering;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

/// A row / document: field name to JSON value.
pub type Row = Map<String, Value>;

/// Comparison applied by a [`Filter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterKind {
    EqualTo,
    NotEqualTo,
    LessThan,
    LessThanOrEqualTo,
    GreaterThan,
    GreaterThanOrEqualTo,
    WhereIn,
    WhereNotIn,
    IsNull,
    IsNotNull,
    Like,
    ArrayContains,
    ArrayContainsAny,
}

impl FilterKind {
    /// Name used by the Masamune workers (`where` query parameter).
    pub fn name(self) -> &'static str {
        match self {
            FilterKind::EqualTo => "equalTo",
            FilterKind::NotEqualTo => "notEqualTo",
            FilterKind::LessThan => "lessThan",
            FilterKind::LessThanOrEqualTo => "lessThanOrEqualTo",
            FilterKind::GreaterThan => "greaterThan",
            FilterKind::GreaterThanOrEqualTo => "greaterThanOrEqualTo",
            FilterKind::WhereIn => "whereIn",
            FilterKind::WhereNotIn => "whereNotIn",
            FilterKind::IsNull => "isNull",
            FilterKind::IsNotNull => "isNotNull",
            FilterKind::Like => "like",
            FilterKind::ArrayContains => "arrayContains",
            FilterKind::ArrayContainsAny => "arrayContainsAny",
        }
    }
}

/// One condition of a [`Query`].
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub kind: FilterKind,
    pub key: String,
    pub value: Option<Value>,
}

impl Serialize for Filter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Filter", 3)?;
        s.serialize_field("type", self.kind.name())?;
        s.serialize_field("key", &self.key)?;
        if let Some(value) = &self.value {
            s.serialize_field("value", value)?;
        }
        s.end()
    }
}

impl Filter {
    pub fn new(kind: FilterKind, key: impl Into<String>, value: Option<Value>) -> Self {
        Self {
            kind,
            key: key.into(),
            value,
        }
    }

    /// Whether `row` satisfies this condition. Missing fields behave like `null`.
    pub fn matches(&self, row: &Row) -> bool {
        let field = row.get(&self.key).unwrap_or(&Value::Null);
        let value = self.value.as_ref().unwrap_or(&Value::Null);
        match self.kind {
            FilterKind::EqualTo => loosely_equal(field, value),
            FilterKind::NotEqualTo => !field.is_null() && !loosely_equal(field, value),
            FilterKind::LessThan => compare(field, value) == Some(Ordering::Less),
            FilterKind::LessThanOrEqualTo => {
                matches!(
                    compare(field, value),
                    Some(Ordering::Less | Ordering::Equal)
                )
            }
            FilterKind::GreaterThan => compare(field, value) == Some(Ordering::Greater),
            FilterKind::GreaterThanOrEqualTo => {
                matches!(
                    compare(field, value),
                    Some(Ordering::Greater | Ordering::Equal)
                )
            }
            FilterKind::WhereIn => value
                .as_array()
                .is_some_and(|values| values.iter().any(|v| loosely_equal(field, v))),
            FilterKind::WhereNotIn => {
                !field.is_null()
                    && value
                        .as_array()
                        .is_none_or(|values| !values.iter().any(|v| loosely_equal(field, v)))
            }
            FilterKind::IsNull => field.is_null(),
            FilterKind::IsNotNull => !field.is_null(),
            FilterKind::Like => match (field.as_str(), value.as_str()) {
                (Some(text), Some(pattern)) => like(text, pattern),
                _ => false,
            },
            FilterKind::ArrayContains => field
                .as_array()
                .is_some_and(|items| items.iter().any(|v| loosely_equal(v, value))),
            FilterKind::ArrayContainsAny => match (field.as_array(), value.as_array()) {
                (Some(items), Some(values)) => items
                    .iter()
                    .any(|item| values.iter().any(|v| loosely_equal(item, v))),
                _ => false,
            },
        }
    }
}

/// Sort key of a [`Query`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Order {
    pub key: String,
    pub descending: bool,
}

/// Conditions, ordering and limit of a collection read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub filters: Vec<Filter>,
    pub order_by: Vec<Order>,
    pub limit: Option<usize>,
}

macro_rules! filter_builder {
    ($($name:ident => $kind:ident),* $(,)?) => {
        $(
            pub fn $name(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
                self.filters.push(Filter::new(FilterKind::$kind, key, Some(value.into())));
                self
            }
        )*
    };
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    filter_builder! {
        equal_to => EqualTo,
        not_equal_to => NotEqualTo,
        less_than => LessThan,
        less_than_or_equal_to => LessThanOrEqualTo,
        greater_than => GreaterThan,
        greater_than_or_equal_to => GreaterThanOrEqualTo,
        where_in => WhereIn,
        where_not_in => WhereNotIn,
        like => Like,
        array_contains => ArrayContains,
        array_contains_any => ArrayContainsAny,
    }

    pub fn is_null(mut self, key: impl Into<String>) -> Self {
        self.filters
            .push(Filter::new(FilterKind::IsNull, key, None));
        self
    }

    pub fn is_not_null(mut self, key: impl Into<String>) -> Self {
        self.filters
            .push(Filter::new(FilterKind::IsNotNull, key, None));
        self
    }

    pub fn order_by(mut self, key: impl Into<String>, descending: bool) -> Self {
        self.order_by.push(Order {
            key: key.into(),
            descending,
        });
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Whether `row` satisfies every filter.
    pub fn matches(&self, row: &Row) -> bool {
        self.filters.iter().all(|f| f.matches(row))
    }

    /// Filters, sorts and limits `rows` the way the server would.
    pub fn apply<T>(&self, rows: Vec<T>, row_of: impl Fn(&T) -> &Row) -> Vec<T> {
        let mut rows: Vec<T> = rows
            .into_iter()
            .filter(|r| self.matches(row_of(r)))
            .collect();
        if !self.order_by.is_empty() {
            rows.sort_by(|a, b| {
                let (a, b) = (row_of(a), row_of(b));
                for order in &self.order_by {
                    let left = a.get(&order.key).unwrap_or(&Value::Null);
                    let right = b.get(&order.key).unwrap_or(&Value::Null);
                    let ordering = compare(left, right).unwrap_or(Ordering::Equal);
                    let ordering = if order.descending {
                        ordering.reverse()
                    } else {
                        ordering
                    };
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                }
                Ordering::Equal
            });
        }
        if let Some(limit) = self.limit {
            rows.truncate(limit);
        }
        rows
    }

    /// `where` query parameter for the Masamune workers (`None` without filters).
    pub fn where_json(&self) -> Option<String> {
        (!self.filters.is_empty()).then(|| serde_json::to_string(&self.filters).unwrap_or_default())
    }

    /// `orderBy` query parameter for the Masamune workers (`None` without ordering).
    pub fn order_json(&self) -> Option<String> {
        (!self.order_by.is_empty())
            .then(|| serde_json::to_string(&self.order_by).unwrap_or_default())
    }
}

/// Equality that treats SQLite booleans (`0`/`1`) and JSON booleans as the same value.
fn loosely_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Bool(x), Value::Number(n)) | (Value::Number(n), Value::Bool(x)) => {
            n.as_f64() == Some(if *x { 1.0 } else { 0.0 })
        }
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Ordering between two JSON values of the same kind (`None` for mixed kinds or `null`).
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64()?.partial_cmp(&y.as_f64()?),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// SQL `LIKE` with `%` and `_`, case-insensitive for ASCII like SQLite.
fn like(text: &str, pattern: &str) -> bool {
    fn go(t: &[char], p: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('%', rest)) => (0..=t.len()).any(|i| go(&t[i..], rest)),
            Some(('_', rest)) => !t.is_empty() && go(&t[1..], rest),
            Some((c, rest)) => {
                t.first().is_some_and(|x| x.eq_ignore_ascii_case(c)) && go(&t[1..], rest)
            }
        }
    }
    let t: Vec<char> = text.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    go(&t, &p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(v: Value) -> Row {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn serializes_like_masamune() {
        let q = Query::new()
            .equal_to("generated", true)
            .is_null("deleted")
            .order_by("date", true);
        assert_eq!(
            q.where_json().unwrap(),
            r#"[{"type":"equalTo","key":"generated","value":true},{"type":"isNull","key":"deleted"}]"#
        );
        assert_eq!(
            q.order_json().unwrap(),
            r#"[{"key":"date","descending":true}]"#
        );
        assert_eq!(Query::new().where_json(), None);
    }

    #[test]
    fn filters_rows() {
        let r = row(json!({"n": 3, "flag": 1, "tags": ["a", "b"], "name": "Hello"}));
        assert!(Filter::new(FilterKind::EqualTo, "flag", Some(json!(true))).matches(&r));
        assert!(Filter::new(FilterKind::GreaterThan, "n", Some(json!(2))).matches(&r));
        assert!(!Filter::new(FilterKind::LessThan, "n", Some(json!(2))).matches(&r));
        assert!(Filter::new(FilterKind::WhereIn, "n", Some(json!([1, 3]))).matches(&r));
        assert!(Filter::new(FilterKind::ArrayContains, "tags", Some(json!("b"))).matches(&r));
        assert!(
            Filter::new(
                FilterKind::ArrayContainsAny,
                "tags",
                Some(json!(["x", "a"]))
            )
            .matches(&r)
        );
        assert!(Filter::new(FilterKind::Like, "name", Some(json!("he%o"))).matches(&r));
        assert!(Filter::new(FilterKind::IsNull, "missing", None).matches(&r));
        assert!(!Filter::new(FilterKind::NotEqualTo, "missing", Some(json!(1))).matches(&r));
    }

    #[test]
    fn applies_order_and_limit() {
        let rows = vec![
            row(json!({"id": "a", "n": 2})),
            row(json!({"id": "b", "n": 3})),
            row(json!({"id": "c", "n": 1})),
        ];
        let q = Query::new()
            .greater_than("n", 1)
            .order_by("n", true)
            .limit(1);
        let out = q.apply(rows, |r| r);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], "b");
    }
}
