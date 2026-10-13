//! Compile-time facts shown in the banner and the generated reference.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // The banner should name the compiler that actually built the binary, not
    // the crate version. `rustc --version` prints something like
    // `rustc 1.92.0 (5f5e0e1c 2026-01-01)`, so keep the whole first word pair.
    let described =
        std::process::Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string()))
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|text| {
                text.split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|text| text.starts_with("rustc"))
            .unwrap_or_else(|| "rustc (unknown)".to_string());

    println!("cargo:rustc-env=VYNGLISH_RUSTC={described}");

    println!("cargo:rustc-env=VYNGLISH_BUILD_TIME={}", build_time());
}

/// The moment the binary was compiled, written for the banner the way CPython
/// writes it: `Oct 11 2026, 14:30:00`. No date library is linked, so the
/// calendar date comes from the civil-from-days algorithm.
fn build_time() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = (secs % 86_400) / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {} {}, {:02}:{:02}:{:02}",
        MONTHS[(month - 1) as usize],
        day,
        year,
        hour,
        minute,
        second
    )
}

/// The calendar date `(year, month, day)` for the number of days since the
/// Unix epoch, using Howard Hinnant's civil-from-days formula.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}
