//! Waiting for a tool process to exit, bounded by a timeout.
//!
//! `std::process::Child` offers only an unbounded `wait` and a non-blocking `try_wait`.
//! Polling `try_wait` on an interval makes every tool invocation pay up to one interval
//! after the tool has already exited, which for a fast formatter or linter is most of
//! the cost of running it. Each platform instead gets a handle that becomes ready when
//! the process exits and can be waited on with a timeout:
//!
//! - Linux: a pidfd (`pidfd_open`), readable once the process has exited.
//! - macOS and FreeBSD: a kqueue with an `EVFILT_PROC`/`NOTE_EXIT` filter.
//! - Windows: the process handle itself, signaled on exit.
//!
//! None of these install signal handlers or touch any other process-wide state, so they
//! are safe in a library that is embedded in a larger program (the LSP server, or any
//! crate using `rumdl_lib`). Where no handle is available (other platforms, or a Linux
//! kernel or sandbox without `pidfd_open`), waiting falls back to polling.
//!
//! The handles only report readiness. The exit status is always collected through
//! `Child::try_wait`, so `Child` stays the one owner that reaps the process and nothing
//! else can race it for the status.

use std::io;
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

/// Interval for the polling fallback.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Wait for `child` to exit, for at most `timeout`.
///
/// Returns `Ok(Some(status))` once the process has exited and been reaped, or `Ok(None)`
/// when `timeout` elapses first, leaving the process running and unreaped for the
/// caller to kill.
pub(super) fn wait_timeout(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        // A timeout too large to represent as a deadline cannot elapse.
        return child.wait().map(Some);
    };
    match ExitNotifier::new(child) {
        Some(notifier) => wait_until(child, deadline, |remaining| notifier.wait(remaining)),
        None => wait_polling(child, deadline),
    }
}

/// Wait by polling `try_wait`, for platforms and environments without an exit handle.
fn wait_polling(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    wait_until(child, deadline, |remaining| {
        std::thread::sleep(remaining.min(POLL_INTERVAL));
        Ok(())
    })
}

/// Alternate between checking for exit and blocking in `block` until `deadline`.
///
/// `block` may return early (a signal interrupted it, or the handle became ready): the
/// loop re-checks the process and the clock each time, so an early return costs one
/// extra `try_wait` rather than a wrong result.
fn wait_until(
    child: &mut Child,
    deadline: Instant,
    mut block: impl FnMut(Duration) -> io::Result<()>,
) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        block(deadline - now)?;
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
use linux::ExitNotifier;

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
use kqueue::ExitNotifier;

#[cfg(windows)]
use windows::ExitNotifier;

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    windows
)))]
use fallback::ExitNotifier;

/// Milliseconds for a `poll`-style timeout, rounded up so a sub-millisecond remainder
/// blocks for one millisecond instead of spinning on a zero timeout.
#[cfg(any(target_os = "linux", target_os = "android", windows))]
fn timeout_millis_ceil(remaining: Duration) -> u128 {
    remaining.as_nanos().div_ceil(1_000_000)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux {
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::process::Child;
    use std::time::Duration;

    /// A pidfd for the child, which `poll` reports readable once the process exits.
    pub(super) struct ExitNotifier {
        pidfd: OwnedFd,
    }

    impl ExitNotifier {
        /// Returns `None` where `pidfd_open` is unavailable (kernels before 5.3, or a
        /// seccomp filter that rejects it), so the caller falls back to polling.
        pub(super) fn new(child: &Child) -> Option<Self> {
            let pid = libc::pid_t::try_from(child.id()).ok()?;
            // SAFETY: `pidfd_open` takes a pid and flags and returns a new descriptor or
            // -1. The child is not yet reaped (only `Child` reaps it, and it has not been
            // waited on), so the pid still names this process and cannot have been reused.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
            if fd < 0 {
                return None;
            }
            // SAFETY: `fd` is a descriptor just returned by the kernel and owned by nothing
            // else.
            Some(Self {
                pidfd: unsafe { OwnedFd::from_raw_fd(fd as libc::c_int) },
            })
        }

        /// Block until the process exits or `remaining` elapses. An interrupted wait
        /// returns early; the caller re-checks.
        pub(super) fn wait(&self, remaining: Duration) -> io::Result<()> {
            let mut fds = libc::pollfd {
                fd: self.pidfd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = libc::c_int::try_from(super::timeout_millis_ceil(remaining)).unwrap_or(libc::c_int::MAX);
            // SAFETY: `fds` is a single valid `pollfd` that outlives the call.
            if unsafe { libc::poll(&raw mut fds, 1, millis) } < 0 {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::Interrupted {
                    return Err(err);
                }
            }
            Ok(())
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
mod kqueue {
    use std::cell::Cell;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::process::Child;
    use std::time::Duration;

    /// Interval for re-checking a child whose exit has been reported but which
    /// `try_wait` cannot collect yet.
    const REAP_INTERVAL: Duration = Duration::from_millis(1);

    /// A kqueue watching the child for exit.
    ///
    /// `NOTE_EXIT` can be delivered a moment before the exited process becomes
    /// reapable, so `try_wait` may still report it running after the event. The event
    /// is one-shot, and blocking on the kqueue again would then sleep until the
    /// deadline. Once the event has arrived, waits re-check on a short interval instead.
    pub(super) struct ExitNotifier {
        kq: OwnedFd,
        exit_reported: Cell<bool>,
    }

    impl ExitNotifier {
        /// Returns `None` when the watch cannot be registered, so the caller falls back
        /// to polling. That includes a child that has already exited: registration on
        /// an exited process can fail with `ESRCH`, and the fallback's first `try_wait`
        /// then collects it at once.
        pub(super) fn new(child: &Child) -> Option<Self> {
            // SAFETY: `kqueue` takes no arguments and returns a new descriptor or -1.
            let raw = unsafe { libc::kqueue() };
            if raw < 0 {
                return None;
            }
            // SAFETY: `raw` is a descriptor just returned by the kernel and owned by
            // nothing else, so it is closed on every return path below.
            let kq = unsafe { OwnedFd::from_raw_fd(raw) };

            // SAFETY: an all-zero `kevent` is a valid value; the fields that matter are
            // set below.
            let mut change: libc::kevent = unsafe { std::mem::zeroed() };
            change.ident = child.id() as _;
            change.filter = libc::EVFILT_PROC;
            change.flags = libc::EV_ADD | libc::EV_ONESHOT;
            change.fflags = libc::NOTE_EXIT;
            // SAFETY: registers one change and reads no events; both pointers are valid
            // for the counts given.
            let registered = unsafe {
                libc::kevent(
                    kq.as_raw_fd(),
                    &raw const change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            };
            (registered == 0).then_some(Self {
                kq,
                exit_reported: Cell::new(false),
            })
        }

        /// Block until the process exits or `remaining` elapses. An interrupted wait
        /// returns early; the caller re-checks.
        pub(super) fn wait(&self, remaining: Duration) -> io::Result<()> {
            if self.exit_reported.get() {
                std::thread::sleep(remaining.min(REAP_INTERVAL));
                return Ok(());
            }
            let timeout = libc::timespec {
                tv_sec: libc::time_t::try_from(remaining.as_secs()).unwrap_or(libc::time_t::MAX),
                tv_nsec: remaining.subsec_nanos() as _,
            };
            // SAFETY: an all-zero `kevent` is a valid buffer for one event.
            let mut event: libc::kevent = unsafe { std::mem::zeroed() };
            // SAFETY: reads at most one event into `event`; all pointers are valid for
            // the duration of the call.
            let received = unsafe {
                libc::kevent(
                    self.kq.as_raw_fd(),
                    std::ptr::null(),
                    0,
                    &raw mut event,
                    1,
                    &raw const timeout,
                )
            };
            if received > 0 {
                self.exit_reported.set(true);
            } else if received < 0 {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::Interrupted {
                    return Err(err);
                }
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

    /// The child's process handle, which is signaled once the process exits.
    pub(super) struct ExitNotifier {
        handle: windows_sys::Win32::Foundation::HANDLE,
    }

    impl ExitNotifier {
        #[expect(
            clippy::unnecessary_wraps,
            reason = "shares the signature of the other platforms' notifiers, which can fail"
        )]
        pub(super) fn new(child: &Child) -> Option<Self> {
            Some(Self {
                handle: child.as_raw_handle(),
            })
        }

        /// Block until the process exits or `remaining` elapses.
        pub(super) fn wait(&self, remaining: Duration) -> io::Result<()> {
            // `INFINITE` is `u32::MAX`, so the largest finite wait is one below it. A
            // longer remainder returns early and the caller waits again.
            let millis =
                u32::try_from(super::timeout_millis_ceil(remaining)).map_or(INFINITE - 1, |ms| ms.min(INFINITE - 1));
            // SAFETY: the handle is owned by `Child`, which outlives this notifier's use
            // in `wait_timeout` and is not closed while it is waited on.
            match unsafe { WaitForSingleObject(self.handle, millis) } {
                WAIT_OBJECT_0 | WAIT_TIMEOUT => Ok(()),
                WAIT_FAILED => Err(io::Error::last_os_error()),
                other => Err(io::Error::other(format!("unexpected wait result {other}"))),
            }
        }
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    windows
)))]
mod fallback {
    use std::io;
    use std::process::Child;
    use std::time::Duration;

    /// No exit handle on this platform, so waiting always polls.
    pub(super) enum ExitNotifier {}

    impl ExitNotifier {
        pub(super) fn new(_child: &Child) -> Option<Self> {
            None
        }

        pub(super) fn wait(&self, _remaining: Duration) -> io::Result<()> {
            match *self {}
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn spawn(script: &str) -> Child {
        Command::new("sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sh should spawn")
    }

    #[test]
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "freebsd"
    ))]
    fn test_platform_has_an_exit_notifier() {
        // Every platform rumdl is released for has a handle; this fails if a cfg gate
        // silently routes one of them to the polling fallback. A Linux kernel or
        // sandbox without `pidfd_open` legitimately has none, so there the notifier is
        // required only when the syscall itself works.
        let mut child = spawn("exec sleep 30");
        let notifier = ExitNotifier::new(&child);
        let expected = platform_supports_notifier(&child);
        child.kill().unwrap();
        child.wait().unwrap();
        if expected {
            assert!(notifier.is_some(), "no exit notifier on this platform");
        }
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn platform_supports_notifier(child: &Child) -> bool {
        // SAFETY: `pidfd_open` takes a pid and flags and returns a new descriptor or -1.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id() as libc::pid_t, 0) };
        if fd < 0 {
            return false;
        }
        // SAFETY: `fd` was just returned by the kernel and is owned by nothing else.
        unsafe { libc::close(fd as libc::c_int) };
        true
    }

    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    fn platform_supports_notifier(_child: &Child) -> bool {
        true
    }

    #[test]
    fn test_returns_the_exit_status_of_a_child_that_exits() {
        let mut child = spawn("exit 7");
        let status = wait_timeout(&mut child, Duration::from_secs(30)).unwrap();
        assert_eq!(status.and_then(|s| s.code()), Some(7));
    }

    #[test]
    fn test_returns_none_and_leaves_the_child_running_at_the_timeout() {
        let mut child = spawn("exec sleep 30");
        let started = Instant::now();
        let status = wait_timeout(&mut child, Duration::from_millis(100)).unwrap();
        let elapsed = started.elapsed();
        assert!(status.is_none(), "expected a timeout, got {status:?}");
        assert!(
            elapsed >= Duration::from_millis(100),
            "returned early after {elapsed:?}"
        );
        // Still running and unreaped, so the caller can kill it.
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn test_collects_a_child_that_exited_before_the_wait_began() {
        let mut child = spawn("exit 3");
        // Give it time to exit, so the watch is registered on an exited process.
        std::thread::sleep(Duration::from_millis(200));
        let status = wait_timeout(&mut child, Duration::from_secs(30)).unwrap();
        assert_eq!(status.and_then(|s| s.code()), Some(3));
    }

    #[test]
    fn test_a_child_exiting_mid_wait_ends_the_wait_before_the_timeout() {
        let mut child = spawn("sleep 0.2; exit 5");
        let started = Instant::now();
        let status = wait_timeout(&mut child, Duration::from_secs(30)).unwrap();
        assert_eq!(status.and_then(|s| s.code()), Some(5));
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "waited {:?}",
            started.elapsed()
        );
    }

    /// An exit reported by the platform before the process is reapable must not leave
    /// the wait blocked until the deadline. The window is narrow, so many short-lived
    /// children are needed to hit it, and a single run catches a regression only some
    /// of the time.
    #[test]
    fn test_every_quick_exit_is_collected_without_waiting_for_the_deadline() {
        for run in 0..1000 {
            let mut child = spawn("exit 0");
            let started = Instant::now();
            let status = wait_timeout(&mut child, Duration::from_secs(30)).unwrap();
            assert_eq!(status.and_then(|s| s.code()), Some(0));
            let waited = started.elapsed();
            assert!(waited < Duration::from_secs(10), "run {run} waited {waited:?}");
        }
    }

    #[test]
    fn test_an_unrepresentable_timeout_waits_for_exit() {
        let mut child = spawn("exit 4");
        let status = wait_timeout(&mut child, Duration::MAX).unwrap();
        assert_eq!(status.and_then(|s| s.code()), Some(4));
    }

    #[test]
    fn test_polling_fallback_returns_status_and_times_out() {
        let mut exits = spawn("exit 6");
        let deadline = Instant::now() + Duration::from_secs(30);
        assert_eq!(
            wait_polling(&mut exits, deadline).unwrap().and_then(|s| s.code()),
            Some(6)
        );

        let mut hangs = spawn("exec sleep 30");
        let deadline = Instant::now() + Duration::from_millis(50);
        assert!(wait_polling(&mut hangs, deadline).unwrap().is_none());
        hangs.kill().unwrap();
        hangs.wait().unwrap();
    }
}
