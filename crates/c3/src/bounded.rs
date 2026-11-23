//! Child processes under ONE deadline (0.2.5).
//!
//! (R2, F13-5) The SessionStart hook's branch line runs its git calls under one deadline; (R4,
//! F16-3) the red-CI check runs every git and `gh` process it starts under one deadline too - 10 s in
//! `c3 finish`, 3 s in the hook - and `c3 finish` runs the repository's own pre-finish checks under
//! one each. They all use this one mechanism: the work runs on a thread of its own bound to a
//! [`ScanChildren`] set ([`within`]); every process it starts through [`run_registered`] is
//! registered in that set until its exit is confirmed (F32-4: both of its pipes reaching their end
//! says nothing about the process - a wrapper may close them and keep running); when the deadline
//! fires, [`ScanChildren::stop`] kills each registered child and sweeps the tree of EVERY child the
//! set started, exited or not (F31-1: a `.cmd` shim that already exited leaves its `ping` behind),
//! reaps what it holds, waits for what it terminated to leave, and refuses every later start - so
//! nothing the work started outlives the caller, and the caller goes on with `None`.
//!
//! The sweep reads the process table ONCE per round ([`tree_to_reap`]): the descendants are found by
//! parent pid (and on Unix by the child's own process group, which an orphan keeps - every
//! registered child starts as the leader of a group of its own), and a process is terminated only
//! when it started at or after the registered child did, and still is that process when it is
//! terminated - never a pid that was reused by something older or unrelated.

use std::collections::HashSet;
use std::io::Read;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::liveness::proc::ScanProc;

/// How long the caller of [`within`] waits, after the stop has run, for the worker thread to end.
const WORKER_GRACE: Duration = Duration::from_millis(1500);

/// How long [`ScanChildren::stop`] waits for the processes it terminated to leave the process table.
const REAP_WAIT: Duration = Duration::from_secs(5);

/// How many times [`ScanChildren::stop`] reads the table and sweeps (a process the tree started
/// between the read and the kill is caught by the next round).
const SWEEP_ROUNDS: usize = 3;

/// One child a [`ScanChildren`] set started, as the stop sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StartedChild {
    pub pid: u32,
    /// Its start time; `None` when it could not be read (its tree is then not swept by pid).
    pub start: Option<DateTime<Utc>>,
    /// When its exit was confirmed and it was reaped; `None` while the set still holds it (its pid
    /// cannot be reused then: the handle - Windows - or the unreaped zombie - Unix - keeps it).
    pub reaped_at: Option<DateTime<Utc>>,
}

/// The child processes of ONE deadline-bound piece of work. While a thread is bound to a set
/// ([`with_scan_children`]), every process started on that thread through [`run_registered`] is
/// registered in it until its exit is confirmed; [`ScanChildren::stop`] - the deadline - kills the
/// registered ones, sweeps the trees of all it started, and refuses every later start. A thread
/// bound to no set (every other caller) runs its command exactly as `Command::output` does.
#[derive(Default)]
pub struct ScanChildren {
    inner: Mutex<ScanChildrenInner>,
}

#[derive(Default)]
struct ScanChildrenInner {
    stopped: bool,
    next_id: u64,
    /// The registered children whose exit is not confirmed yet.
    running: Vec<(u64, std::process::Child)>,
    /// Every child ever started through this set.
    started: Vec<StartedChild>,
}

impl ScanChildrenInner {
    fn mark_reaped(&mut self, pid: u32) {
        let now = Utc::now();
        for s in self.started.iter_mut().filter(|s| s.pid == pid) {
            if s.reaped_at.is_none() {
                s.reaped_at = Some(now);
            }
        }
    }
}

/// A process's start time from the live process table, `None` when it cannot be read.
fn start_of(pid: u32) -> Option<DateTime<Utc>> {
    let iso = crate::liveness::proc::process_start_iso(pid)?;
    DateTime::parse_from_rfc3339(&iso)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Whether `pid` still is the process that started at `created` (an unreadable start time is no).
fn still_same(pid: u32, created: DateTime<Utc>) -> bool {
    start_of(pid) == Some(created)
}

/// The process group of a table row (Unix: field 5 of `/proc/<pid>/stat`; Windows has none).
fn group_of(p: &ScanProc) -> Option<u32> {
    crate::liveness::proc::process_group(p.pid)
}

/// The processes a stop terminates, from ONE read of the process table (pure): for every child the
/// set started whose start time is known,
///
/// * the rows whose parent pid is that child's, started at or after it did - and, for a child
///   already reaped, not after it was reaped (a later one has a process that reused the pid for its
///   parent);
/// * (Unix, `group_of`) the rows in the child's own process group, started at or after it did - an
///   orphan keeps its group - unless the pid is held by another process now (then the group, if
///   any, is that process's: a group id is not given out again while the group has members);
///
/// then, from every row taken, its own children started at or after it did, to the bottom of the
/// tree. A row without a readable start time is never taken, nor a child the set started itself
/// (the stop holds those by their handle).
pub(crate) fn tree_to_reap(
    table: &[ScanProc],
    roots: &[StartedChild],
    group_of: impl Fn(&ScanProc) -> Option<u32>,
) -> Vec<(u32, DateTime<Utc>)> {
    let mut seen: HashSet<u32> = roots.iter().map(|r| r.pid).collect();
    let mut out: Vec<(u32, DateTime<Utc>)> = Vec::new();
    let mut frontier: Vec<(u32, DateTime<Utc>)> = Vec::new();
    for r in roots {
        let Some(start) = r.start else { continue };
        let pid_reused = r.reaped_at.is_some() && table.iter().any(|p| p.pid == r.pid);
        for p in table {
            let Some(created) = p.created else { continue };
            if seen.contains(&p.pid) || created < start {
                continue;
            }
            let child = p.ppid == r.pid && r.reaped_at.is_none_or(|t| created <= t);
            let grouped = !pid_reused && group_of(p) == Some(r.pid);
            if (child || grouped) && seen.insert(p.pid) {
                out.push((p.pid, created));
                frontier.push((p.pid, created));
            }
        }
    }
    while let Some((parent, parent_start)) = frontier.pop() {
        for p in table {
            let Some(created) = p.created else { continue };
            if p.ppid == parent && created >= parent_start && seen.insert(p.pid) {
                out.push((p.pid, created));
                frontier.push((p.pid, created));
            }
        }
    }
    out
}

impl ScanChildren {
    fn lock(&self) -> std::sync::MutexGuard<'_, ScanChildrenInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The deadline: refuse every later start; then, in up to [`SWEEP_ROUNDS`] rounds, read the
    /// process table once, kill every registered child by its handle (first round) and terminate
    /// every process of the trees of ALL the children the set started ([`tree_to_reap`]) that
    /// still is the process the table showed; reap the registered children; and wait (at most
    /// [`REAP_WAIT`]) until everything terminated has left. When the table cannot be read at all,
    /// each registered child's tree is killed from its pid (`taskkill /T` on Windows, its process
    /// group elsewhere). Returns when all of that has run.
    pub fn stop(&self) {
        let mut g = self.lock();
        g.stopped = true;
        let mut terminated: Vec<(u32, DateTime<Utc>)> = Vec::new();
        for round in 0..SWEEP_ROUNDS {
            let table = crate::liveness::proc::enumerate_processes_checked();
            let victims = match &table {
                Ok(t) => tree_to_reap(t, &g.started, group_of),
                Err(_) => Vec::new(),
            };
            if round == 0 {
                for (_, child) in g.running.iter_mut() {
                    if table.is_err() && matches!(child.try_wait(), Ok(None)) {
                        crate::engines::subprocess::kill_tree_by_pid(child.id());
                        kill_group(child.id());
                    }
                    let _ = child.kill();
                }
            }
            let mut any = false;
            for &(pid, created) in &victims {
                if still_same(pid, created) {
                    crate::liveness::proc::terminate_pid(pid);
                    any = true;
                    if !terminated.contains(&(pid, created)) {
                        terminated.push((pid, created));
                    }
                }
            }
            if !any {
                break;
            }
        }
        let held: Vec<(u64, std::process::Child)> = g.running.drain(..).collect();
        for (_, mut child) in held {
            let _ = child.wait();
            let pid = child.id();
            g.mark_reaped(pid);
        }
        drop(g);
        let until = std::time::Instant::now() + REAP_WAIT;
        while terminated.iter().any(|&(p, c)| still_same(p, c)) && std::time::Instant::now() < until
        {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// (tests) The pids of the children whose exit is not confirmed yet.
    #[cfg(test)]
    pub(crate) fn running_pids(&self) -> Vec<u32> {
        self.lock().running.iter().map(|(_, c)| c.id()).collect()
    }

    /// (tests) The pid of every child ever started through this set.
    #[cfg(test)]
    pub(crate) fn started_pids(&self) -> Vec<u32> {
        self.lock().started.iter().map(|s| s.pid).collect()
    }
}

/// (Unix) Kill the process group `pgid` (a registered child leads its own); a no-op on Windows.
fn kill_group(pgid: u32) {
    #[cfg(unix)]
    {
        if pgid > 1 {
            let _ = Command::new("kill")
                .args(["-9", "--", &format!("-{pgid}")])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    #[cfg(not(unix))]
    let _ = pgid;
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
/// pipes, the child (on Unix) leads a process group of its own, it stays registered until its exit
/// is confirmed, and a stop kills it (`Err`); a start after a stop is refused (`Err`, nothing is
/// started).
pub(crate) fn run_registered(cmd: Command) -> std::io::Result<Output> {
    run_registered_with(cmd, true)
}

/// [`run_registered`]; `pipes: false` (tests) gives the child no stdout / stderr at all - as a
/// wrapper that closed both and keeps running looks to the reader.
fn run_registered_with(mut cmd: Command, pipes: bool) -> std::io::Result<Output> {
    let Some(set) = SCAN_CHILDREN.with(|s| s.borrow().clone()) else {
        return cmd.output();
    };
    cmd.stdin(Stdio::null());
    if pipes {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
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
        let pid = child.id();
        g.started.push(StartedChild {
            pid,
            start: start_of(pid),
            reaped_at: None,
        });
        g.running.push((id, child));
        (id, out, err)
    };
    // stderr on a helper thread, stdout here: both end when the child (and whatever inherited the
    // pipes) closes them, exits or is killed
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
    let status = wait_registered(&set, id)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// (F32-4) Wait for the registered child `id` to exit WITHOUT taking it out of the set first: it is
/// polled under the lock and leaves the registry only once its exit is confirmed, so a stop that
/// fires meanwhile still kills it. `Err` when the stop took it (killed and reaped it).
fn wait_registered(set: &ScanChildren, id: u64) -> std::io::Result<ExitStatus> {
    let mut pause = Duration::from_millis(1);
    loop {
        {
            let mut g = set.lock();
            let Some(pos) = g.running.iter().position(|(i, _)| *i == id) else {
                return Err(stopped_error());
            };
            match g.running[pos].1.try_wait() {
                Ok(Some(status)) => {
                    let (_, child) = g.running.swap_remove(pos);
                    g.mark_reaped(child.id());
                    return Ok(status);
                }
                Ok(None) => {}
                // it stays registered: a stop still kills it
                Err(e) => return Err(e),
            }
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}

/// `f()` on a thread of its own bound to a fresh [`ScanChildren`]: its result when it is ready
/// within `deadline`; else every child it started is stopped ([`ScanChildren::stop`] - it returns
/// once its sweep has run to its end) and the answer is `None`. The worker thread - whose children
/// are gone and whose next start is refused - is then given [`WORKER_GRACE`] to end and joined when
/// it does; one still busy past that is left behind (its result is never used; the process ends
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
    let worker = std::thread::Builder::new()
        .name("c3-bounded".to_string())
        .spawn(move || {
            let _ = tx.send(with_scan_children(bound, f));
        })
        .ok()?;
    match rx.recv_timeout(deadline) {
        Ok(v) => {
            let _ = worker.join();
            v
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // the worker ended without an answer (it panicked)
            let _ = worker.join();
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            children.stop();
            if rx.recv_timeout(WORKER_GRACE).is_ok() || worker.is_finished() {
                let _ = worker.join();
            }
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

    /// Wait (at most 10 s) until none of `pids` is in the process table; the ones still there.
    fn still_alive_after_wait(pids: &[u32]) -> Vec<u32> {
        let until = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let alive: Vec<u32> = pids
                .iter()
                .copied()
                .filter(|&p| crate::liveness::proc::process_start_iso(p).is_some())
                .collect();
            if alive.is_empty() || std::time::Instant::now() >= until {
                return alive;
            }
            std::thread::sleep(Duration::from_millis(100));
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

    // (F32-4) A child whose pipes are at their end (here: it has none) but which keeps running
    // stays registered until it exits: the deadline still kills it, and nothing of it outlives the
    // caller of `within`.
    #[test]
    fn a_child_stays_under_the_deadline_until_it_has_exited() {
        let children = Arc::new(ScanChildren::default());
        let t = std::time::Instant::now();
        let out = within_stopping(
            Duration::from_millis(1500),
            || run_registered_with(sleeper(), false).ok().map(|_| ()),
            Arc::clone(&children),
        );
        assert_eq!(out, None);
        assert!(t.elapsed() < Duration::from_secs(12), "{:?}", t.elapsed());
        let started = children.started_pids();
        assert_eq!(started.len(), 1, "the sleeper never started");
        assert!(children.running_pids().is_empty());
        assert_eq!(
            still_alive_after_wait(&started),
            Vec::<u32>::new(),
            "the child outlived the deadline"
        );
    }

    /// A shim that starts a long-lived grandchild (its output not on the shim's pipes) and exits at
    /// once: a `.cmd` on Windows, `sh -c` elsewhere.
    fn exiting_shim(dir: &std::path::Path) -> Command {
        if cfg!(windows) {
            let p = dir.join("shim.cmd");
            std::fs::write(
                &p,
                "@echo off\r\nstart \"\" /b ping -n 30 127.0.0.1 >nul 2>nul\r\nexit /b 0\r\n",
            )
            .unwrap();
            Command::new(p)
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", "sleep 30 >/dev/null 2>&1 </dev/null & exit 0"]);
            c
        }
    }

    // (F31-1) A shim that already exited - started and reaped through the set - leaves a
    // grandchild running; when the deadline fires, the stop sweeps the shim's tree too: after
    // `within` returns, nothing of that tree is alive.
    #[test]
    fn the_deadline_reaps_what_an_exited_shim_left_behind() {
        let dir = std::env::temp_dir().join(format!(
            "c3-bounded-shim-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let children = Arc::new(ScanChildren::default());
        // watch the tree while it runs: the shim's pid, then its child once it shows (by parent
        // pid, or - an orphan on Unix - by the shim's process group)
        let watch = Arc::clone(&children);
        let watcher = std::thread::spawn(move || {
            let until = std::time::Instant::now() + Duration::from_secs(4);
            while std::time::Instant::now() < until {
                if let Some(&shim) = watch.started_pids().first() {
                    let found: Vec<u32> = crate::liveness::proc::enumerate_processes()
                        .iter()
                        .filter(|p| p.pid != shim)
                        .filter(|p| {
                            p.ppid == shim
                                || crate::liveness::proc::process_group(p.pid) == Some(shim)
                        })
                        .map(|p| p.pid)
                        .collect();
                    if !found.is_empty() {
                        return found;
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Vec::new()
        });
        let dir_w = dir.clone();
        let out = within_stopping(
            Duration::from_secs(5),
            move || {
                // the shim exits at once (and is reaped, or - while its child holds the pipes -
                // waits registered); the work goes on past the deadline
                let _ = run_registered(exiting_shim(&dir_w));
                std::thread::sleep(Duration::from_secs(20));
                Some(())
            },
            Arc::clone(&children),
        );
        assert_eq!(out, None);
        let tree = watcher.join().unwrap();
        assert!(!tree.is_empty(), "the shim's grandchild was never seen");
        assert!(children.running_pids().is_empty());
        let alive = still_alive_after_wait(&tree);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            alive.is_empty(),
            "outlived the deadline: {alive:?} of {tree:?}"
        );
    }

    fn row(pid: u32, ppid: u32, at: i64) -> ScanProc {
        ScanProc {
            pid,
            ppid,
            name: String::new(),
            created: DateTime::from_timestamp(at, 0),
            command_line: String::new(),
        }
    }

    fn root(pid: u32, at: i64, reaped: Option<i64>) -> StartedChild {
        StartedChild {
            pid,
            start: DateTime::from_timestamp(at, 0),
            reaped_at: reaped.and_then(|t| DateTime::from_timestamp(t, 0)),
        }
    }

    fn pids(v: &[(u32, DateTime<Utc>)]) -> Vec<u32> {
        let mut p: Vec<u32> = v.iter().map(|x| x.0).collect();
        p.sort();
        p
    }

    // (F31-1) The sweep takes the tree of every child the set started - running or reaped - and
    // never a process older than the child, a child of a process that reused a reaped child's pid,
    // or a row without a start time.
    #[test]
    fn the_sweep_takes_only_what_the_children_started() {
        let table = vec![
            row(100, 1, 1000),   // a held child itself (killed by its handle, not swept)
            row(101, 100, 1001), // its child
            row(102, 101, 1002), // its grandchild
            row(103, 101, 990),  // "child" of 101 older than 101: 101's pid was reused before
            row(104, 100, 999),  // older than the root: not its child
            row(200, 1, 2000),   // pid 200 reused (the child 200 was reaped at 1600)
            row(201, 200, 2001), // the reuser's child: not swept
            row(202, 200, 1550), // the reaped child's own child, started before it was reaped
            row(203, 202, 1560), // and its child
            ScanProc {
                created: None,
                ..row(105, 100, 0)
            }, // no start time: never taken
        ];
        let roots = [root(100, 1000, None), root(200, 1500, Some(1600))];
        let got = tree_to_reap(&table, &roots, |_| None);
        assert_eq!(pids(&got), vec![101, 102, 202, 203]);
        // a child whose start time is unknown is not swept by pid
        let got = tree_to_reap(&table, &[root(100, 0, None)], |_| None);
        assert!(pids(&got).contains(&101));
        let unknown = StartedChild {
            pid: 100,
            start: None,
            reaped_at: None,
        };
        assert!(tree_to_reap(&table, &[unknown], |_| None).is_empty());
    }

    // (F31-1, Unix) An orphan - re-parented after its parent exited - is found by the child's
    // process group; not when the child's pid is held by another process now.
    #[test]
    fn an_orphan_is_found_by_its_process_group() {
        let table = vec![
            row(301, 1, 1001),   // orphaned grandchild of the reaped child 300 (group 300)
            row(302, 301, 1700), // its child, started after 300 was reaped
            row(310, 1, 1002),   // an unrelated orphan of another group
        ];
        let group = |p: &ScanProc| match p.pid {
            301 | 302 => Some(300),
            _ => Some(p.pid),
        };
        let got = tree_to_reap(&table, &[root(300, 1000, Some(1500))], group);
        assert_eq!(pids(&got), vec![301, 302]);
        // pid 300 held by another process now: its group is not the child's
        let mut reused = table.clone();
        reused.push(row(300, 1, 1600));
        let got = tree_to_reap(&reused, &[root(300, 1000, Some(1500))], group);
        assert!(got.is_empty(), "{got:?}");
    }
}
