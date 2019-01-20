
#[cfg(unix)]
mod sig {
    use libc::{c_int, c_void, sighandler_t, signal};
    use std::cell::RefCell;
    use std::sync::atomic::{Ordering, AtomicUsize, ATOMIC_USIZE_INIT};
    use std::thread_local;

    static SIGINFO_RECEIVED: AtomicUsize = ATOMIC_USIZE_INIT;
    thread_local! {
        static SIGINFO_GEN: RefCell<usize> = RefCell::new(0);
    }

    extern "C" fn trigger_signal(_: c_int) {
        SIGINFO_RECEIVED.fetch_add(1, Ordering::Release);
    }

    fn get_handler() -> sighandler_t {
        trigger_signal as extern "C" fn(c_int) as *mut c_void as sighandler_t
    }

    pub(crate) fn check_signal() -> bool {
        SIGINFO_GEN.with(|gen| {
            let current = SIGINFO_RECEIVED.load(Ordering::Acquire);
            let received = current != *gen.borrow();
            *gen.borrow_mut() = current;
            received
        })
    }

    pub(crate) fn hook_signal() {
        unsafe {
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "bitrig"
            ))]
            signal(libc::SIGINFO, get_handler());

            signal(libc::SIGUSR1, get_handler());
        }
    }
}


#[cfg(not(unix))]
mod sig {
    pub(crate) fn check_signal() -> bool {
        false
    }

    pub(crate) fn hook_signal() {}
}

pub(crate) use sig::*;
