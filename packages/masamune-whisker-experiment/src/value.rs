use std::fmt;

/// A typed value stored in [`crate::ExperimentConfig`], defaults, or overrides.
#[derive(Clone, Debug, PartialEq)]
pub enum ExperimentValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ExperimentValue {
    pub fn as_str_value(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::String(value) => value.clone(),
        }
    }
}

impl fmt::Display for ExperimentValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::String(value) => f.write_str(value),
        }
    }
}

impl From<bool> for ExperimentValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for ExperimentValue {
    fn from(value: i32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<i64> for ExperimentValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f32> for ExperimentValue {
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}

impl From<f64> for ExperimentValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for ExperimentValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for ExperimentValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<&String> for ExperimentValue {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}
