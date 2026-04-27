use std::time::Instant;

pub fn enabled() -> bool {
    std::env::var("DEVNEST_PERF_LOG")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub fn log_elapsed(label: &str, started_at: Instant) {
    if enabled() {
        eprintln!(
            "DevNest perf {label}: {}ms",
            started_at.elapsed().as_millis()
        );
    }
}
