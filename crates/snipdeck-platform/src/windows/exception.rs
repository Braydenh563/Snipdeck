//! A last-chance exception filter, so a native crash (an access violation in a driver or a
//! shell extension, a heap corruption) leaves a report like a panic does. A panic has its hook in
//! the app's `crash` module; the operating system's exceptions bypass it.
//!
//! [`install`] takes the function that writes the report. The filter calls it with the exception
//! code and the faulting address, then returns `EXCEPTION_CONTINUE_SEARCH` so Windows still shows
//! its own crash dialog and Windows Error Reporting still runs. The handler runs in a damaged
//! process: it should do as little as possible (no locks it might not get).
//!
//! **Unverified on Windows**: compiles for the Windows target only; the owner must force a crash
//! on Windows to see that the report is written. A stack overflow on the main thread is stopped by
//! Rust's own guard-page handler first and may not reach this filter.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};

/// What to call with the exception code and the faulting address.
pub type Handler = fn(code: u32, address: usize);

/// The handler, as an address (0 = none). An atomic, not a lock: the filter must not wait.
static HANDLER: AtomicUsize = AtomicUsize::new(0);

/// `EXCEPTION_CONTINUE_SEARCH`: let Windows go on with its own handling.
const CONTINUE_SEARCH: i32 = 0;

/// Makes `handler` run when an exception nothing else handled ends the process.
pub fn install(handler: Handler) {
    HANDLER.store(handler as usize, Ordering::SeqCst);
    // SAFETY: `filter` has the signature Windows expects, lives for the whole process and reads
    // only the record Windows hands it. The previous filter (none by default) is dropped.
    unsafe {
        SetUnhandledExceptionFilter(Some(filter));
    }
}

unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
    let address = HANDLER.load(Ordering::SeqCst);
    // SAFETY: Windows passes a valid `EXCEPTION_POINTERS` whose record is valid for this call.
    let record = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) };
    if let (true, Some(record)) = (address != 0, record) {
        // SAFETY: `address` is a `Handler` stored by `install` (nothing else writes `HANDLER`).
        let handler: Handler = unsafe { std::mem::transmute::<usize, Handler>(address) };
        handler(
            record.ExceptionCode.0.cast_unsigned(),
            record.ExceptionAddress as usize,
        );
    }
    CONTINUE_SEARCH
}
