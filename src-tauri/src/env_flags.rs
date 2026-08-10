//! Opt-in switches read from the environment.
//!
//! The WinUI OverlayHost surfaces are experimental and stay off unless a flag
//! turns them on. The entry point decides whether to spawn the host at all and
//! the runtime adapters decide whether to talk to it, so both have to agree on
//! what counts as "on" — hence one reader rather than a copy in each.

/// Whether `name` is set to a value a human would read as "yes".
///
/// Unset, empty, and unrecognised values are all false: an experiment stays
/// off unless it was deliberately enabled.
pub fn env_truthy(name: &str) -> bool {
    std::env::var(name).map(|v| is_truthy(&v)).unwrap_or(false)
}

/// The reading itself, separated from the lookup so it can be tested without
/// mutating the process environment out from under parallel tests.
fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accepted_spellings_are_on() {
        for value in ["1", "true", "TRUE", "Yes", " on "] {
            assert!(is_truthy(value), "expected {value:?} to read as on");
        }
    }

    #[test]
    fn anything_else_is_off() {
        for value in ["", "  ", "0", "false", "no", "off", "enabled", "2"] {
            assert!(!is_truthy(value), "expected {value:?} to read as off");
        }
    }

    #[test]
    fn an_unset_variable_is_off() {
        assert!(!env_truthy(
            "MEOWCAL_A_VARIABLE_THIS_PROCESS_WILL_NEVER_SET"
        ));
    }
}
