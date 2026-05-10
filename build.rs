use std::process::Command;

fn main() {
    let git_tag = env_or_git(
        "GIT_TAG",
        &["describe", "--tags", "--always", "--dirty"],
        "unknown",
    );
    let git_hash = env_or_git("GIT_HASH", &["rev-parse", "--short=12", "HEAD"], "unknown");
    let build_date = std::env::var("BUILD_DATE")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(utc_now_iso8601);

    println!("cargo:rustc-env=GIT_TAG={git_tag}");
    println!("cargo:rustc-env=GIT_HASH={git_hash}");
    println!("cargo:rustc-env=BUILD_DATE={build_date}");

    println!("cargo:rerun-if-env-changed=GIT_TAG");
    println!("cargo:rerun-if-env-changed=GIT_HASH");
    println!("cargo:rerun-if-env-changed=BUILD_DATE");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-changed=build.rs");
}

fn env_or_git(var: &str, args: &[&str], fallback: &str) -> String {
    if let Ok(v) = std::env::var(var) {
        if !v.is_empty() {
            return v;
        }
    }
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn utc_now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_to_iso8601_utc(secs)
}

fn format_unix_to_iso8601_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let secs_of_day = (secs % 86_400) as u32;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}
