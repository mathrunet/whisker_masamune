# masamune-whisker-experiment

Remote Config backed A/B tests and feature switches for Whisker apps, ported from Flutter's
`masamune_experiment`.

## Usage

1. Register [`ExperimentMasamuneAdapter`] in `MasamuneAdapters`:

```rust,ignore
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_experiment::ExperimentMasamuneAdapter;

let experiment_adapter = if is_test {
    ExperimentMasamuneAdapter::test()
        .override_value("new_button", "control")
} else {
    ExperimentMasamuneAdapter::firebase()
        .default_value("new_button", "control")
};

MasamuneAdapters::new()
    .adapter(experiment_adapter)
    .install();
```

2. Create an [`Experiment`] handle (for example in `AppState`), load values at startup (or push
   already-fetched Remote Config values with [`Experiment::apply_values`]), and branch with
   [`Experiment::run`]:

```rust,ignore
use masamune_whisker_experiment::{
    Experiment, ExperimentCase, ExperimentExposure, RunOptions,
};

let experiment = Experiment::primary();
let variant = experiment
    .run(
        |config| config.get_string("new_button"),
        [
            ExperimentCase::equals("treatment", "treatment", || Variant::Treatment),
            ExperimentCase::otherwise("control", || Variant::Control),
        ],
        RunOptions::new().exposure(ExperimentExposure::new("ab_slot_1", "F-20261127-02")),
    )
    .unwrap_or(Variant::Control);
```

When `exposure` is provided and values have been loaded without hitting an override or `fixed`,
`Experiment::run` emits `ExperimentLoggerEvent.exposure` once per `(user_property, value)` pair
per process. `masamune-whisker-logger-firebase` automatically records that event as a Firebase
Analytics user property (`<experiment_id>:<label>`).
