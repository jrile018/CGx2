//! Opt-in, best-effort diagnostic timing. Never part of a trust decision.
use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

pub(crate) struct Scope {
    start: Option<Instant>,
    phase: &'static str,
    pub(crate) bytes: u64,
}

impl Scope {
    pub(crate) fn new(phase: &'static str) -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| {
            std::env::var_os("CGX2_LOADER_TRACE").is_some_and(|value| value == "1")
        });
        Self {
            start: enabled.then(Instant::now),
            phase,
            bytes: 0,
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let ns = start.elapsed().as_nanos();
            // A closed diagnostic stream must not replace a verification error.
            let _ = writeln!(
                std::io::stderr().lock(),
                "cgx2-loader pid={} phase={} ns={} bytes={}",
                std::process::id(),
                self.phase,
                ns,
                self.bytes
            );
        }
    }
}
