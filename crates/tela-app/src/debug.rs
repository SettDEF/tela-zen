//! Debug channels: Wine's grammar, output worth reading.
//!
//! Extension problems are nearly all invisible. A message that never arrives, a
//! port nobody answered, a page store that was never built — none of it fails
//! loudly. The extension simply does less than it should, which reads as "the
//! extension is broken" rather than as a specific missing call.
//!
//! `TELA_DEBUG` takes the grammar `WINEDEBUG` uses, because it is a good one:
//!
//! ```text
//! TELA_DEBUG=+port                 every class on the port channel
//! TELA_DEBUG=trace+port,+event     trace on port, everything on event
//! TELA_DEBUG=+all,-storage         everything except storage
//! TELA_DEBUG=warn+all              warnings everywhere
//! ```
//!
//! The output is not Wine's. `class:channel:function message` is greppable and
//! nothing else; a log read while chasing a race needs to answer *when* and
//! *how long since* at a glance:
//!
//! ```text
//!    0.284s  trace  port   ublock-origin   connect      portId=ublock:1 tab=3
//!  + 0.121s  trace  port   ublock-origin   accept       listeners=1
//!  + 1.884s  err    page   ublock-origin   contentscript.js:412  TypeError: …
//! ```
//!
//! Elapsed since start, then the gap since the last line *on that channel* —
//! which is what makes a stall obvious rather than something you compute by
//! subtracting timestamps. Colour when stderr is a terminal, never when piped.
//!
//! `err` and `fixme` are on by default; `warn` and `trace` are not. A `fixme`
//! means the shim knowingly did less than a real browser would, which is the
//! most useful thing to see when an extension misbehaves.

use std::fmt;
use std::io::IsTerminal;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Err,
    Fixme,
    Warn,
    Trace,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Err => "err",
            Class::Fixme => "fixme",
            Class::Warn => "warn",
            Class::Trace => "trace",
        }
    }

    /// SGR parameters, chosen so severity reads by colour alone.
    fn colour(self) -> &'static str {
        match self {
            Class::Err => "1;31",
            Class::Fixme => "1;35",
            Class::Warn => "33",
            Class::Trace => "2;37",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "err" => Some(Class::Err),
            "fixme" => Some(Class::Fixme),
            "warn" => Some(Class::Warn),
            "trace" => Some(Class::Trace),
            _ => None,
        }
    }

    /// What prints when `TELA_DEBUG` says nothing about it.
    fn on_by_default(self) -> bool {
        matches!(self, Class::Err | Class::Fixme)
    }

    fn all() -> [Class; 4] {
        [Class::Err, Class::Fixme, Class::Warn, Class::Trace]
    }
}

/// One `[class][+-]channel` term, kept in written order: later terms win, so
/// `+all,-storage` reads as it looks.
struct Rule {
    class: Option<Class>,
    channel: String,
    enable: bool,
}

fn rules() -> &'static Vec<Rule> {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| match std::env::var("TELA_DEBUG") {
        Ok(spec) => parse(&spec),
        Err(_) => Vec::new(),
    })
}

fn parse(spec: &str) -> Vec<Rule> {
    let mut out = Vec::new();
    for term in spec.split(',') {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }

        // `class+channel` / `class-channel`, or a bare `+channel` / `channel`.
        let (class, rest) = match term.find(['+', '-']) {
            Some(0) | None => (None, term),
            Some(at) => match Class::parse(&term[..at]) {
                Some(class) => (Some(class), &term[at..]),
                // Not a class prefix, so the whole term is a channel name that
                // happens to contain a dash.
                None => (None, term),
            },
        };

        let (enable, channel) = match rest.strip_prefix('+') {
            Some(name) => (true, name),
            None => match rest.strip_prefix('-') {
                Some(name) => (false, name),
                // A bare name enables, as in Wine.
                None => (true, rest),
            },
        };

        if !channel.is_empty() {
            out.push(Rule { class, channel: channel.to_owned(), enable });
        }
    }
    out
}

fn resolve(rules: &[Rule], class: Class, channel: &str) -> bool {
    let mut on = class.on_by_default();
    for rule in rules {
        if rule.class.is_some_and(|only| only != class) {
            continue;
        }
        if rule.channel == "all" || rule.channel == channel {
            on = rule.enable;
        }
    }
    on
}

/// Whether this class/channel pair prints.
pub fn enabled(class: Class, channel: &str) -> bool {
    resolve(rules(), class, channel)
}

/// True when any class on the channel prints — for skipping work that exists
/// only to be logged.
pub fn channel_enabled(channel: &str) -> bool {
    Class::all().into_iter().any(|class| enabled(class, channel))
}

fn started() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn colouring() -> bool {
    static COLOUR: OnceLock<bool> = OnceLock::new();
    *COLOUR.get_or_init(|| {
        // Piped output is read by grep and by people pasting into issues;
        // escape codes help neither.
        std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
    })
}

/// Last line's timestamp per channel, for the gap column.
fn marks() -> &'static Mutex<Vec<(String, Instant)>> {
    static MARKS: OnceLock<Mutex<Vec<(String, Instant)>>> = OnceLock::new();
    MARKS.get_or_init(|| Mutex::new(Vec::new()))
}

fn since_last_on(channel: &str, now: Instant) -> Option<Duration> {
    let mut marks = marks().lock().ok()?;
    match marks.iter_mut().find(|(name, _)| name == channel) {
        Some((_, last)) => {
            let gap = now.duration_since(*last);
            *last = now;
            Some(gap)
        }
        None => {
            marks.push((channel.to_owned(), now));
            None
        }
    }
}

fn paint(text: &str, sgr: &str) -> String {
    if colouring() {
        format!("\x1b[{sgr}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

/// Pad to exactly `width`, or cut to it with an ellipsis.
///
/// Exactly is the point: a grid that a long extension id can push out of
/// alignment is not a grid, and extension ids are routinely long.
fn fit(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return format!("{text}{}", " ".repeat(width - count));
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// One formatted line, without the trailing newline.
///
/// Split out from printing so its shape is testable: alignment is the whole
/// point of the format, and alignment is exactly what silently rots.
fn format_line(
    elapsed: Duration,
    gap: Option<Duration>,
    class: Class,
    channel: &str,
    subject: &str,
    func: &str,
    message: &str,
    colour: bool,
) -> String {
    let paint = |text: &str, sgr: &str| -> String {
        if colour { format!("\x1b[{sgr}m{text}\x1b[0m") } else { text.to_owned() }
    };

    let at = format!("{:>7.3}s", elapsed.as_secs_f64());
    // Only worth showing once there is something to compare against, and only
    // when the gap is big enough to mean anything.
    let gap = match gap {
        Some(gap) if gap >= Duration::from_millis(1) => {
            format!("+{:>6.3}s", gap.as_secs_f64())
        }
        _ => " ".repeat(8),
    };

    format!(
        "{} {} {} {} {} {} {}",
        paint(&at, "2;37"),
        paint(&gap, "2;36"),
        paint(&fit(class.name(), 5), class.colour()),
        paint(&fit(channel, 7), "36"),
        paint(&fit(subject, 15), "1"),
        paint(&fit(func, 20), "2;37"),
        message
    )
}

/// Print one line, if its channel is on.
///
/// `subject` is what the line is *about* — an extension id, a tab — so a log
/// can be narrowed to one actor with grep. Pass `""` when there isn't one.
pub fn log_about(class: Class, channel: &str, subject: &str, func: &str, args: fmt::Arguments) {
    if !enabled(class, channel) {
        return;
    }
    banner();
    let now = Instant::now();
    let elapsed = now.duration_since(started());
    let gap = since_last_on(channel, now);
    eprintln!(
        "{}",
        format_line(
            elapsed,
            gap,
            class,
            channel,
            subject,
            func,
            &args.to_string(),
            colouring(),
        )
    );
}

pub fn log(class: Class, channel: &str, func: &str, args: fmt::Arguments) {
    log_about(class, channel, "", func, args);
}

/// Said once, before the first line: which channels are live.
///
/// Without it, a quiet log is ambiguous — nothing happened, or nothing was
/// being watched? This distinguishes them.
fn banner() {
    static BANNER: OnceLock<()> = OnceLock::new();
    BANNER.get_or_init(|| {
        let spec = std::env::var("TELA_DEBUG").unwrap_or_default();
        if spec.is_empty() {
            return;
        }
        let mut live: Vec<String> = Vec::new();
        for rule in rules() {
            if rule.enable && !live.contains(&rule.channel) {
                live.push(rule.channel.clone());
            }
        }
        eprintln!(
            "{} {} {}",
            paint("tela:", "1"),
            paint("debug channels", "2;37"),
            paint(&live.join(" "), "36")
        );
        eprintln!(
            "{}",
            paint(
                "       elapsed  gap      class channel subject         where",
                "2;37"
            )
        );
    });
}

#[macro_export]
macro_rules! tela_debug {
    ($class:expr, $chan:expr, $func:expr, $($arg:tt)*) => {
        $crate::debug::log($class, $chan, $func, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! trace {
    ($chan:expr, $func:expr, $($arg:tt)*) => {
        $crate::tela_debug!($crate::debug::Class::Trace, $chan, $func, $($arg)*)
    };
}

#[macro_export]
macro_rules! fixme {
    ($chan:expr, $func:expr, $($arg:tt)*) => {
        $crate::tela_debug!($crate::debug::Class::Fixme, $chan, $func, $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::{Class, Duration, enabled, format_line, parse, resolve};

    fn on(spec: &str, class: Class, channel: &str) -> bool {
        resolve(&parse(spec), class, channel)
    }

    #[test]
    fn defaults_print_problems_and_stay_quiet_otherwise() {
        // Nothing configured: the classes meaning "something is wrong" print,
        // the running commentary does not.
        assert!(enabled(Class::Err, "port"));
        assert!(enabled(Class::Fixme, "port"));
        assert!(!enabled(Class::Trace, "port"));
        assert!(!enabled(Class::Warn, "port"));
    }

    #[test]
    fn a_bare_channel_enables_every_class_on_it() {
        assert!(on("+port", Class::Trace, "port"));
        assert!(on("port", Class::Trace, "port"));
        assert!(!on("+port", Class::Trace, "storage"));
    }

    #[test]
    fn a_class_prefix_narrows_to_that_class() {
        assert!(on("trace+port", Class::Trace, "port"));
        assert!(!on("trace+port", Class::Warn, "port"));
    }

    #[test]
    fn later_terms_win_so_all_minus_one_reads_as_written() {
        assert!(on("+all,-storage", Class::Trace, "port"));
        assert!(!on("+all,-storage", Class::Trace, "storage"));
        // Order matters, as in Wine: the last word on a channel is kept.
        assert!(on("-all,+port", Class::Trace, "port"));
        assert!(!on("-all,+port", Class::Trace, "event"));
    }

    #[test]
    fn junk_is_ignored_rather_than_disabling_everything() {
        assert!(on(",,  ,+port", Class::Trace, "port"));
        assert!(!on("nonsense", Class::Trace, "port"));
    }

    #[test]
    fn columns_line_up_between_lines() {
        let short = format_line(
            Duration::from_millis(284), None, Class::Err, "port", "ublock",
            "connect", "up", false,
        );
        let long = format_line(
            Duration::from_millis(1284), Some(Duration::from_millis(121)),
            Class::Trace, "storage", "adaptive-tab-bar", "retrieveParameters",
            "down", false,
        );
        // The message is what varies; everything before it is a fixed grid, so
        // the two lines must agree on where the message starts. Measured in
        // characters, not bytes: the column that overflowed is truncated with
        // an ellipsis, which is one column wide and three bytes long.
        let column_of = |line: &str, needle: &str| {
            let at = line.find(needle).expect("message present");
            line[..at].chars().count()
        };
        assert_eq!(column_of(&short, "up"), column_of(&long, "down"));

        // And the overflowing subject really was cut rather than allowed to
        // push the grid sideways.
        assert!(long.contains("adaptive-tab-b…"));
    }

    #[test]
    fn a_gap_only_appears_once_there_is_one_worth_reading() {
        let first = format_line(
            Duration::from_secs(1), None, Class::Trace, "port", "x", "f", "m", false,
        );
        assert!(!first.contains('+'));

        // Sub-millisecond gaps are noise, not information.
        let instant = format_line(
            Duration::from_secs(1), Some(Duration::from_micros(40)),
            Class::Trace, "port", "x", "f", "m", false,
        );
        assert!(!instant.contains('+'));

        let stalled = format_line(
            Duration::from_secs(2), Some(Duration::from_millis(1884)),
            Class::Trace, "port", "x", "f", "m", false,
        );
        assert!(stalled.contains("+ 1.884s"));
    }

    #[test]
    fn colour_is_opt_in_so_piped_output_stays_plain() {
        let plain = format_line(
            Duration::from_secs(1), None, Class::Err, "port", "x", "f", "m", false,
        );
        assert!(!plain.contains('\x1b'));

        let painted = format_line(
            Duration::from_secs(1), None, Class::Err, "port", "x", "f", "m", true,
        );
        assert!(painted.contains('\x1b'));
    }
}
