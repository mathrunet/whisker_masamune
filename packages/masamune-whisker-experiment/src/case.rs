/// One branch of [`crate::Experiment::run`].
///
/// Cases are evaluated in order. The first case whose `when` predicate returns `true` runs its
/// `run` closure and becomes the return value of [`crate::Experiment::run`].
pub struct ExperimentCase<V, R> {
    label: String,
    when: Box<dyn Fn(&V) -> bool>,
    run: Box<dyn FnOnce() -> R>,
}

impl<V, R> ExperimentCase<V, R> {
    /// Creates a branch with a custom predicate.
    pub fn new(
        label: impl Into<String>,
        when: impl Fn(&V) -> bool + 'static,
        run: impl FnOnce() -> R + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            when: Box::new(when),
            run: Box::new(run),
        }
    }

    /// Creates a branch that matches when the value equals `expected`.
    pub fn equals(
        expected: impl Into<V>,
        label: impl Into<String>,
        run: impl FnOnce() -> R + 'static,
    ) -> Self
    where
        V: PartialEq + 'static,
    {
        let expected = expected.into();
        Self::new(label, move |value| *value == expected, run)
    }

    /// Creates a fallback branch that always matches. Place it at the end for the control group.
    pub fn otherwise(label: impl Into<String>, run: impl FnOnce() -> R + 'static) -> Self {
        Self::new(label, |_| true, run)
    }

    /// Name of the branch (for example `"control"` or `"treatment"`).
    pub fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn matches(&self, value: &V) -> bool {
        (self.when)(value)
    }

    pub(crate) fn execute(self) -> R {
        (self.run)()
    }
}

impl<V, R> std::fmt::Debug for ExperimentCase<V, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExperimentCase").field(&self.label).finish()
    }
}
