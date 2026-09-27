//! Regenerate the desktop palette from a colour picked in the browser.
//!
//! This is the one place Tela runs something on the system, so the boundary is
//! drawn tightly and stated plainly:
//!
//! - **Off by default.** `integration.systemTheme.enabled` must be set.
//! - **Allowlisted caller.** Only the extension id in
//!   `integration.systemTheme.extension` may ask. A web page cannot reach this
//!   at all, and nor can any other extension.
//! - **No shell.** Commands are spawned directly with an argument vector; there
//!   is no string a caller could inject a `;` or `$(...)` into.
//! - **Validated argument.** The seed must match `#RRGGBB` exactly. Everything
//!   else — output paths, the binary name — is fixed here, not supplied.
//!
//! What it deliberately is *not*: a way to run an arbitrary command. Adding one
//! would turn the browser into a persistence mechanism, which is the line this
//! project does not cross.

use std::path::PathBuf;
use std::process::Command;

/// A seed colour, already proven to be `#RRGGBB`.
pub struct Seed(String);

impl Seed {
    /// Reject anything that is not exactly a six-digit hex colour.
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
        if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Seed(format!("#{}", digits.to_ascii_lowercase())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn state_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(base.join("quickshell/user/generated"))
}

/// Where the desktop's own apply script lives, if the user pointed at one.
fn apply_script(configured: &str) -> Option<PathBuf> {
    if configured.is_empty() {
        return None;
    }
    let home = std::env::var("HOME").ok();
    let path = PathBuf::from(expand_home(configured, home.as_deref()));
    // Must exist and be a file we can execute — a missing script is a
    // misconfiguration, not something to guess around.
    if path.is_file() { Some(path) } else { None }
}

/// Expand a leading `~/`. Takes the home directory rather than reading the
/// environment so it can be tested without mutating process state — which
/// Rust 2024 makes unsafe, and which would race other tests anyway.
pub(crate) fn expand_home(value: &str, home: Option<&str>) -> String {
    match (value.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => format!("{home}/{rest}"),
        _ => value.to_owned(),
    }
}

/// Outcome, so the caller can say what happened rather than just "false".
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Applied,
    Disabled,
    NotAllowed,
    BadSeed,
    GenerateFailed(String),
}

impl Outcome {
    pub fn message(&self) -> String {
        match self {
            Outcome::Applied => "Palette regenerated".to_owned(),
            Outcome::Disabled => {
                "System theming is off (integration.systemTheme.enabled)".to_owned()
            }
            Outcome::NotAllowed => "This extension may not change system colours".to_owned(),
            Outcome::BadSeed => "Seed must be #RRGGBB".to_owned(),
            Outcome::GenerateFailed(detail) => format!("tinct failed: {detail}"),
        }
    }
}

/// Run the generator, then hand off to the desktop's apply script.
pub fn apply(seed: &Seed, script: &str) -> Outcome {
    let Some(state) = state_dir() else {
        return Outcome::GenerateFailed("no state directory".to_owned());
    };
    if std::fs::create_dir_all(&state).is_err() {
        return Outcome::GenerateFailed("state directory not writable".to_owned());
    }

    let colors = state.join("colors.json");
    let scss = state.join("material_colors.scss");

    // Spawned directly: no shell, so the seed cannot be anything but one
    // argument, and it has already been validated to six hex digits.
    let generated = Command::new("tinct")
        .arg("generate")
        .arg("--color")
        .arg(seed.as_str())
        .arg("--out-json")
        .arg(&colors)
        .arg("--out-scss")
        .arg(&scss)
        .output();

    match generated {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Outcome::GenerateFailed(detail.lines().next().unwrap_or("failed").to_owned());
        }
        Err(error) => return Outcome::GenerateFailed(error.to_string()),
    }

    // The desktop's own script renders templates and reloads apps. Optional:
    // without it the palette is still written, and anything watching
    // colors.json — including Tela itself — picks it up.
    if let Some(script) = apply_script(script) {
        let _ = Command::new(&script).spawn();
    }

    Outcome::Applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_six_digit_hex() {
        assert_eq!(Seed::parse("#A1B2C3").unwrap().as_str(), "#a1b2c3");
        assert_eq!(Seed::parse("a1b2c3").unwrap().as_str(), "#a1b2c3");
        assert_eq!(Seed::parse("  #ffffff  ").unwrap().as_str(), "#ffffff");
    }

    #[test]
    fn rejects_anything_else() {
        // Short, long, non-hex, and — the ones that matter — attempts to smuggle
        // shell syntax through. Even without a shell these must not reach argv.
        for bad in [
            "", "#fff", "#1234567", "#gggggg", "red",
            "#aaaaaa; rm -rf /", "$(id)", "`id`", "--out-json=/etc/passwd",
            "#aaaaaa\n#bbbbbb",
        ] {
            assert!(Seed::parse(bad).is_none(), "should reject {bad:?}");
        }
    }

    #[test]
    fn expands_a_leading_tilde_only() {
        assert_eq!(expand_home("~/x/y.sh", Some("/home/test")), "/home/test/x/y.sh");
        // A tilde anywhere else is a literal path component.
        assert_eq!(expand_home("/opt/~weird", Some("/home/test")), "/opt/~weird");
        // No home available: leave it alone rather than guessing.
        assert_eq!(expand_home("~/x", None), "~/x");
    }

    #[test]
    fn a_missing_script_is_not_used() {
        assert!(apply_script("/definitely/not/here.sh").is_none());
        assert!(apply_script("").is_none());
    }
}
