//! Accessibility semantics shared by the widgets.
//!
//! UI automation (Maestro, XCUITest, UIAutomator) finds elements by their accessibility
//! identifier and label. Every interactive widget carries a label and a role, plus the
//! identifier when the caller passes one: on iOS a node becomes an accessibility element only
//! when it has a label, hint or role, so an identifier alone would not be visible to Maestro.

use whisker::prelude::*;

/// Semantics of an element with `role`, labelled `label` and optionally identified by
/// `identifier`.
pub(crate) fn semantics(
    role: AccessibilityRole,
    identifier: Option<&str>,
    label: impl Into<String>,
    state: AccessibilityState,
) -> Accessibility {
    let accessibility = Accessibility::new().label(label).role(role).state(state);
    match identifier {
        Some(identifier) => accessibility.identifier(identifier),
        None => accessibility,
    }
}

/// Semantics of a button; `disabled` is announced and exposed to UI automation.
pub(crate) fn button(
    identifier: Option<&str>,
    label: impl Into<String>,
    disabled: bool,
) -> Accessibility {
    semantics(
        AccessibilityRole::Button,
        identifier,
        label,
        AccessibilityState::new().disabled(disabled),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_carries_identifier_label_role_and_state() {
        let actual = button(Some("home_increment"), "Increment", true);
        assert_eq!(actual.identifier.as_deref(), Some("home_increment"));
        assert_eq!(actual.label.as_deref(), Some("Increment"));
        assert_eq!(actual.role, Some(AccessibilityRole::Button));
        assert_eq!(actual.state.disabled, Some(true));
    }

    #[test]
    fn button_without_identifier_keeps_label_and_role() {
        let actual = button(None, "Back", false);
        assert_eq!(actual.identifier, None);
        assert_eq!(actual.label.as_deref(), Some("Back"));
        assert_eq!(actual.role, Some(AccessibilityRole::Button));
        assert_eq!(actual.state.disabled, Some(false));
    }

    #[test]
    fn tab_carries_selected_state() {
        let actual = semantics(
            AccessibilityRole::Tab,
            Some("tab_home"),
            "Home",
            AccessibilityState::new().selected(true),
        );
        assert_eq!(actual.role, Some(AccessibilityRole::Tab));
        assert_eq!(actual.state.selected, Some(true));
        assert_eq!(actual.identifier.as_deref(), Some("tab_home"));
    }
}
