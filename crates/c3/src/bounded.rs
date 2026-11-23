//! Child processes under ONE deadline (0.2.5).
//!
//! (R2, F13-5) The SessionStart hook's branch line runs its git calls under one deadline; (R4,
//! F16-3) the red-CI check runs every git and `gh` process it starts under one deadline too - 10 s in
//! `c3 finish`, 3 s in the hook. Both use this one mechanism: the work runs on a thread of its own
//! bound to a [`ScanChildren`] set ([`within`]); every process it starts through [`run_registered`]
//! is registered in that set for as long as it runs; when the deadline fires, [`ScanChildren::stop`]
//! kills each registered child together with its descendants (a `.cmd` shim in front of the real
//! program, a `gh` that runs git), reaps it, and refuses every later start - so nothing the work
//! started outlives the caller, and the caller goes on with `None`.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The child processes of ONE deadline-bound piece of work. While a thread is bound to a set
/// ([`with_scan_children`]), every process started on that thread through [`run_registered`] is
/// registered in it for as long as it runs; [`ScanChildren::stop`] - the deadline - kills the
/// running ones and refuses every later start. A thread bound to no set (every other caller) runs
/// its command exactly as `Command::output` does.
#[derive(Default)]
pub struct ScanChildren {
    inner: Mutex<ScanChildrenInner>,
}

#[derive(Default)]
struct ScanChildrenInner {
    stopped: bool,
    next_id: u64,
    running: Vec<(u64, std::process::Child)>,
    /// (tests) The pid of every child ever started through this set.
    #[cfg(test)]
    started: Vec<u32>,
}

impl ScanChildren {
    fn lock(&self) -> std::sync::MutexGuard<'_, ScanChildrenInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Kill every registered child that still runs - its descendants first (Windows `taskkill /T`,
    /// elsewhere each descendant by pid), then the child itself by its handle - reap it, and refuse
    /// every later start.
    pub fn stop(&self) {
        let mut g = self.lock();
        g.stopped = true;
        for (_, mut child) in g.running.drain(..) {
            if matches!(child.try_wait(), Ok(None)) {
                crate::engines::subprocess::kill_tree_by_pid(child.id());
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// (tests) The pids of the children running now.
    #[cfg(test)]
    pub(crate) fn running_pids(&self) -> Vec<u32> {
        self.lock().running.iter().map(|(_, c)| c.id()).collect()
    }

    /// (tests) The pid of every child ever started through this set.
    #[cfg(test)]
    pub(crate) fn started_pids(&self) -> Vec<u32> {
        self.lock().started.clone()
    }
}

thread_local! {
    /// The [`ScanChildren`] this thread's children are registered in, if any.
    static SCAN_CHILDREN: std::cell::RefCell<Option<Arc<ScanChildren>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` on this thread with `set` bound: every child started meanwhile through
/// [`run_registered`] is registered in it (and stopped with it).
pub fn with_scan_children<T>(set: Arc<ScanChildren>, f: impl FnOnce() -> T) -> T {
    SCAN_CHILDREN.with(|s| *s.borrow_mut() = Some(set));
    let out = f();
    SCAN_CHILDREN.with(|s| *s.borrow_mut() = None);
    out
}

/// The error of a start refused, or a child killed, by [`ScanChildren::stop`].
fn stopped_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "stopped by the deadline")
}

/// Run `cmd` to its end and collect its output, as `Command::output` does - registered in this
/// thread's [`ScanChildren`] when one is bound: then stdin is null, stdout and stderr are read from
/// pipes, and a stop kills the child (`Err`); a start after a stop is refused (`Err`, nothing is
/// started).
pub(crate) fn run_registered(mut cmd: Command) -> std::io::Result<Output> {
    let Some(set) = SCAN_CHILDREN.with(|s| s.borrow().clone()) else {
        return cmd.output();
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (id, out_pipe, err_pipe) = {
        let mut g = set.lock();
        if g.stopped {
            return Err(stopped_error());
        }
        // started under the lock: a stop either sees this child or this start sees the stop
        let mut child = cmd.spawn()?;
        let out = child.stdout.take();
        let err = child.stderr.take();
        g.next_id += 1;
        let id = g.next_id;
        #[cfg(test)]
        g.started.push(child.id());
        g.running.push((id, child));
        (id, out, err)
    };
    // stderr on a helper thread, stdout here: both end when the child exits or is killed
    let err_reader = err_pipe.map(|mut e| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = e.read_to_end(&mut b);
            b
        })
    });
    let mut stdout = Vec::new();
    if let Some(mut o) = out_pipe {
        let _ = o.read_to_end(&mut stdout);
    }
    let stderr = err_reader.and_then(|h| h.join().ok()).unwrap_or_default();
    let child = {
        let mut g = set.lock();
        let pos = g.running.iter().position(|(i, _)| *i == id);
        pos.map(|p| g.running.swap_remove(p).1)
    };
    match child {
        Some(mut c) => Ok(Output {
            status: c.wait()?,
            stdout,
            stderr,
        }),
        // the stop took it, killed and reaped it
        None => Err(stopped_error()),
    }
}

/// `f()` on a thread of its own bound to a fresh [`ScanChildren`]: its result when it is ready
/// within `deadline`; else every child it started is stopped ([`ScanChildren::stop`]) and the
/// answer is `None` - the thread itself is left behind (its result is never used; the process ends
/// without waiting for it).
pub fn within<T: Send + 'static>(
    deadline: Duration,
    f: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    within_stopping(deadline, f, Arc::new(ScanChildren::default()))
}

/// [`within`] with the set given by the caller (a test reads its pids afterwards).
pub fn within_stopping<T: Send + 'static>(
    deadline: Duration,
    f: impl FnOnce() -> Option<T> + Send + 'static,
    children: Arc<ScanChildren>,
) -> Option<T> {
    // A child of work that misses the deadline must not keep this process's own output open (a
    // reader that waits for end-of-stream - Claude Code collecting the hook's output - would wait
    // for it): this process's standard handles are made non-inheritable first, as the detached
    // maintenance pass does. A child's own stdout/stderr pipes are not affected.
    std_handles_not_inheritable();
    let bound = Arc::clone(&children);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("c3-bounded".to_string())
        .spawn(move || {
            let _ = tx.send(with_scan_children(bound, f));
        })
        .ok()?;
    match rx.recv_timeout(deadline) {
        Ok(v) => v,
        Err(_) => {
            children.stop();
            None
        }
    }
}

/// Make this process's standard handles non-inheritable (Windows; elsewhere a no-op): a child
/// started later keeps only the handles it is given explicitly.
fn std_handles_not_inheritable() {
    #[cfg(windows)]
    {
        const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;
        const STD_HANDLES: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
        extern "system" {
            fn GetStdHandle(n: u32) -> isize;
            fn SetHandleInformation(h: isize, mask: u32, flags: u32) -> i32;
        }
        for n in STD_HANDLES {
            // SAFETY: GetStdHandle takes a constant and returns a handle value (or 0 / -1); a
            // valid handle is only re-flagged, never closed.
            unsafe {
                let h = GetStdHandle(n);
                if h != 0 && h != -1 {
                    SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A child that sleeps for 30 s (a cross-platform sleeper, as the liveness tests use).
    fn sleeper() -> Command {
        if cfg!(windows) {
            let mut c = Command::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        }
    }

    // (R2, F13-5) A stopped set kills the child it is running (no process outlives the stop) and
    // refuses every later start; a thread bound to no set runs its command plainly.
    #[test]
    fn a_stopped_scan_kills_its_running_child_and_starts_no_other() {
        let set = Arc::new(ScanChildren::default());
        let bound = Arc::clone(&set);
        let worker =
            std::thread::spawn(move || with_scan_children(bound, || run_registered(sleeper())));
        let until = std::time::Instant::now() + Duration::from_secs(20);
        let pid = loop {
            if let Some(p) = set.running_pids().first() {
                break *p;
            }
            assert!(
                std::time::Instant::now() < until,
                "the sleeper never started"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let t = std::time::Instant::now();
        set.stop();
        let r = worker.join().unwrap();
        assert!(r.is_err(), "a stopped child is an error: {r:?}");
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert!(
            crate::liveness::proc::process_start_iso(pid).is_none(),
            "the child {pid} outlived the stop"
        );
        assert!(set.running_pids().is_empty());
        // a later start is refused: nothing is started
        let bound = Arc::clone(&set);
        let r = std::thread::spawn(move || {
            with_scan_children(bound, || {
                let mut g = Command::new("git");
                g.arg("--version");
                run_registered(g)
            })
        })
        .join()
        .unwrap();
        assert!(r.is_err());
        assert_eq!(set.started_pids(), vec![pid]);
        // unbound: as Command::output
        let mut g = Command::new("git");
        g.arg("--version");
        assert!(run_registered(g).unwrap().status.success());
        // bound and not stopped: the output is collected as Command::output collects it
        let fresh = Arc::new(ScanChildren::default());
        let out = with_scan_children(Arc::clone(&fresh), || {
            let mut g = Command::new("git");
            g.arg("--version");
            run_registered(g)
        })
        .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("git version"));
        assert!(fresh.running_pids().is_empty());
    }
}
