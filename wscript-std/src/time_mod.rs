//! `time` — clocks, sleeping and timestamp formatting (PRD §7 follow-up).
//! **Capability**: registering this module grants scripts clock access and
//! the ability to block the thread (`sleep`).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wscript_core::Module;

/// Process-wide anchor for the monotonic clock: `instant()` returns
/// seconds since the first call in the process, so instants are plain
/// floats scripts can subtract.
static ANCHOR: OnceLock<Instant> = OnceLock::new();

fn monotonic_secs() -> f64 {
    ANCHOR.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub fn time() -> Module {
    let mut m = Module::new("time");
    m.doc("Clocks and sleeping (capability: clock access, thread blocking)");

    m.fn_("now_unix", || -> f64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(),
        }
    });
    m.fn_("now_millis", || -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_millis() as i64,
            Err(e) => -(e.duration().as_millis() as i64),
        }
    });
    // Monotonic clock: `let t = time::instant()` … `time::elapsed(t)`.
    // Instants are seconds (float) since a process-wide anchor.
    m.fn_("instant", || -> f64 { monotonic_secs() });
    m.fn_("elapsed", |start: f64| -> f64 { monotonic_secs() - start });
    // Blocks the VM thread; negative durations clamp to zero. Not
    // interruptible by fuel (fuel meters instructions, not host time).
    m.fn_("sleep", |ms: i64| {
        std::thread::sleep(Duration::from_millis(ms.max(0) as u64));
    });
    // UTC ISO-8601 from a unix timestamp (seconds): "1970-01-01T00:00:00Z".
    // Sub-second precision is truncated.
    m.fn_("format_iso", |ts: f64| -> String { format_iso(ts) });
    m
}

fn format_iso(ts: f64) -> String {
    let secs = ts.floor() as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Days-from-civil inverse (Howard Hinnant's algorithm): unix day count →
/// (year, month, day) in the proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_epoch() {
        assert_eq!(format_iso(0.0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso_known_timestamps() {
        // 2000-02-29 leap day, 12:34:56 UTC.
        assert_eq!(format_iso(951_827_696.0), "2000-02-29T12:34:56Z");
        // 2026-01-01T00:00:00Z.
        assert_eq!(format_iso(1_767_225_600.0), "2026-01-01T00:00:00Z");
        // Pre-epoch: 1969-12-31T23:59:59Z.
        assert_eq!(format_iso(-1.0), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn civil_roundtrip_scan() {
        // Every day across several leap boundaries maps monotonically.
        let mut prev = civil_from_days(-1);
        for day in 0..80_000 {
            let cur = civil_from_days(day);
            assert!(cur > prev, "day {day}: {cur:?} !> {prev:?}");
            assert!((1..=12).contains(&cur.1));
            assert!((1..=31).contains(&cur.2));
            prev = cur;
        }
    }
}
