pub mod changelog;
pub mod client;

use std::cell::Cell;

// Thread-local flag: when true, `record_change()` is a no-op.
// Set this to `true` while applying remote sync changes to avoid
// creating changelog entries for server-originated mutations.
thread_local! {
    static SYNC_APPLYING: Cell<bool> = const { Cell::new(false) };
}

/// Run a closure with sync tracking disabled.
/// Use when applying remote changes to the local DB.
pub fn with_sync_disabled<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    SYNC_APPLYING.with(|flag| {
        let was = flag.replace(true);
        let result = f();
        flag.set(was);
        result
    })
}

/// Returns true if we are currently applying remote sync changes.
pub fn is_sync_applying() -> bool {
    SYNC_APPLYING.with(|flag| flag.get())
}
