use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global shutdown flag the GUI toggles to stop the monitor thread.
pub static SHUTDOWN: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));

pub fn should_shutdown() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

pub fn request_stop() {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

pub fn reset() {
    SHUTDOWN.store(false, Ordering::SeqCst);
}