#![doc = include_str!("../README.md")]

mod adapter;
mod case;
mod config;
mod experiment;
mod exposure;
mod source;
mod value;

pub use adapter::ExperimentMasamuneAdapter;
pub use case::ExperimentCase;
pub use config::ExperimentConfig;
pub use experiment::{Experiment, RunOptions};
pub use exposure::{ExperimentExposure, ExperimentLoggerEvent};
#[cfg(feature = "firebase-remote-config")]
pub use source::FirebaseRemoteConfigSource;
pub use source::{ExperimentError, ExperimentSource, LoadFuture, RuntimeExperimentSource};
pub use value::ExperimentValue;
