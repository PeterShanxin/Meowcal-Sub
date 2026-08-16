//! Which port browser dev mode listens on.
//!
//! A fixed 3001 made browser verification a shared-machine hazard: the port was
//! either free or the run failed, and the only remedy on offer was to find out
//! what else on the machine held it and stop it - someone else's process, on
//! someone else's project. `MEOWCAL_HTTP_PORT` lets a harness name a port it has
//! already confirmed is free (#35).
//!
//! Its own module rather than a few lines in `http_server.rs`, because that file
//! is at its reviewed ceiling and this is a separate rule with its own tests.

/// The port browser dev mode listens on unless something says otherwise.
pub const DEFAULT_BROWSER_PORT: u16 = 3001;

/// The port `--http-only` binds, given a configured value.
///
/// `0` is accepted and means "let the OS choose", which only helps a caller that
/// reads the bound address back out of the log.
///
/// A malformed value is refused rather than silently defaulted: a harness that
/// set the variable and got 3001 anyway would collide with the very process it
/// was avoiding, and the collision would read as a flake rather than as the
/// configuration error it is.
pub fn resolve_browser_port(configured: Option<&str>) -> Result<u16, String> {
    let Some(raw) = configured else {
        return Ok(DEFAULT_BROWSER_PORT);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_BROWSER_PORT);
    }
    trimmed
        .parse::<u16>()
        .map_err(|_| format!("MEOWCAL_HTTP_PORT must be a port number 0-65535, got {trimmed:?}"))
}

/// The same decision, reading the process environment.
pub fn browser_port_from_environment() -> Result<u16, String> {
    resolve_browser_port(std::env::var("MEOWCAL_HTTP_PORT").ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_or_empty_value_keeps_the_default() {
        assert_eq!(resolve_browser_port(None), Ok(DEFAULT_BROWSER_PORT));
        assert_eq!(resolve_browser_port(Some("")), Ok(DEFAULT_BROWSER_PORT));
        assert_eq!(resolve_browser_port(Some("   ")), Ok(DEFAULT_BROWSER_PORT));
    }

    #[test]
    fn a_configured_port_is_used() {
        assert_eq!(resolve_browser_port(Some("49152")), Ok(49152));
        assert_eq!(resolve_browser_port(Some(" 8080 ")), Ok(8080));
    }

    #[test]
    fn zero_means_the_operating_system_chooses() {
        assert_eq!(resolve_browser_port(Some("0")), Ok(0));
    }

    #[test]
    fn a_malformed_value_is_refused_rather_than_defaulted() {
        for value in ["-1", "70000", "3001x", "eighty"] {
            let error = resolve_browser_port(Some(value))
                .expect_err("expected a malformed port to be refused");
            assert!(
                error.contains("MEOWCAL_HTTP_PORT"),
                "error should name the variable, got {error}"
            );
        }
    }
}
