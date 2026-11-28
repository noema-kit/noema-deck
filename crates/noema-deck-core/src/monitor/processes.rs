//! Every process on the machine, read from `ps`.
//!
//! `ps` may read the processes of every user, which Deck may not: asked
//! directly, macOS tells a program nothing about the window server or the
//! system's own services, which are often the busiest. `ps` reports the
//! processor time a process has used so far, so its share of a processor is
//! the time it used between two readings.
//!
//! For memory `ps` knows only the part resident in memory now, which leaves
//! out what the system has compressed or swapped out, often most of a large
//! program. The kernel tells the whole footprint, as Activity Monitor shows
//! it, but only of the reader's own processes.

use std::{
    collections::HashMap,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use libproc::pid_rusage::{RUsageInfoV0, pidrusage};
use serde::Serialize;

use crate::pane::PaneId;

/// What `ps` prints for each process. The command comes last, as a path may
/// hold spaces.
const FIELDS: &str = "pid=,ppid=,user=,rss=,time=,comm=";
/// The most steps from a process up to the pane it was started in. A parent
/// starts before its child, so the walk ends by itself unless the list was
/// read while it changed.
const DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    /// The program's file name.
    pub name: String,
    pub user: String,
    /// The share of one processor used since the last reading, in percent, so
    /// a process on two processors can reach 200.
    pub cpu: f64,
    /// The memory it holds, in bytes.
    pub memory: u64,
    /// Whether that is only the part resident in memory now, which is all
    /// that can be told of another user's process.
    pub partial: bool,
    /// The pane it was started in, by that pane's process or one it started.
    pub pane: Option<PaneId>,
}

/// One line of `ps`.
#[derive(Debug, PartialEq)]
struct Row {
    pid: u32,
    parent: u32,
    user: String,
    memory: u64,
    partial: bool,
    /// The processor time used so far, in seconds.
    spent: f64,
    name: String,
}

impl Row {
    fn parse(line: &str) -> Option<Self> {
        let mut rest = line;
        let mut field = || {
            let (field, after) = rest.trim_start().split_once(char::is_whitespace)?;
            rest = after;
            Some(field)
        };
        let (pid, parent, user, memory, spent) = (field()?, field()?, field()?, field()?, field()?);
        let name = rest.trim().rsplit('/').next().filter(|name| !name.is_empty())?;
        Some(Self {
            pid: pid.parse().ok()?,
            parent: parent.parse().ok()?,
            user: user.into(),
            // `ps` counts memory in units of 1,024 bytes.
            memory: memory.parse::<u64>().ok()? * 1024,
            partial: true,
            spent: seconds(spent)?,
            name: name.into(),
        })
    }
}

impl Row {
    /// The row with the process's whole footprint, where the kernel tells it.
    fn whole(mut self) -> Self {
        let usage = i32::try_from(self.pid).map_err(|_| String::new()).and_then(pidrusage);
        if let Ok(RUsageInfoV0 { ri_phys_footprint, .. }) = usage {
            (self.memory, self.partial) = (ri_phys_footprint, false);
        }
        self
    }
}

/// A time as `ps` writes it, such as `4294:34.04` or `1-02:03:04`, in seconds.
fn seconds(time: &str) -> Option<f64> {
    let (days, clock) = match time.split_once('-') {
        Some((days, clock)) => (days.parse::<f64>().ok()?, clock),
        None => (0.0, time),
    };
    let clock =
        clock.split(':').try_fold(0.0, |sum, part| Some(sum * 60.0 + part.parse::<f64>().ok()?))?;
    Some(days * 86400.0 + clock)
}

/// The processes, and the processor time each had used at the last reading.
#[derive(Default)]
pub(super) struct Processes {
    spent: HashMap<u32, f64>,
    /// When `ps` last answered. The time between two answers is what the
    /// processor time gained is a share of, however long the rest of a
    /// reading takes.
    read: Option<Instant>,
}

impl Processes {
    /// Reads every process, busiest first.
    pub(super) fn read(&mut self) -> Result<Vec<Process>> {
        let ps = Command::new("ps")
            .args(["-A", "-o", FIELDS])
            // So the time is written with a point, whatever the locale.
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Run `ps` to list the processes")?;
        let own = ps.id();
        let output = ps.wait_with_output()?;
        ensure!(output.status.success(), "`ps` could not list the processes");
        let now = Instant::now();
        let elapsed = self.read.replace(now).map_or(Duration::ZERO, |read| now - read);
        let text = String::from_utf8_lossy(&output.stdout);
        let rows = text.lines().filter_map(Row::parse).filter(|row| row.pid != own);
        Ok(self.take(rows.map(Row::whole), elapsed))
    }

    /// Turns a reading into processes, each with the share of a processor it
    /// used over `elapsed`. A process not seen before has used none yet.
    fn take(&mut self, rows: impl Iterator<Item = Row>, elapsed: Duration) -> Vec<Process> {
        let seconds = elapsed.as_secs_f64();
        let mut spent = HashMap::new();
        let mut processes: Vec<_> = rows
            .map(|row| {
                let before = self.spent.get(&row.pid).copied();
                spent.insert(row.pid, row.spent);
                let cpu = match before {
                    Some(before) if seconds > 0.0 => (row.spent - before).max(0.0) / seconds,
                    _ => 0.0,
                };
                Process {
                    pid: row.pid,
                    parent: row.parent,
                    name: row.name,
                    user: row.user,
                    // Kept to a tenth of a percent, which is all `ps` can tell.
                    cpu: (cpu * 1000.0).round() / 10.0,
                    memory: row.memory,
                    partial: row.partial,
                    pane: None,
                }
            })
            .collect();
        self.spent = spent;
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
        processes
    }
}

/// Marks each process started in a pane with that pane: the pane's own
/// process, given in `roots`, and everything started under it. A process that
/// outlives the one that started it belongs to the system from then on, and
/// to no pane.
pub(super) fn attribute(processes: &mut [Process], roots: &[(PaneId, u32)]) {
    if roots.is_empty() {
        return;
    }
    let parents: HashMap<u32, u32> =
        processes.iter().map(|process| (process.pid, process.parent)).collect();
    for process in processes {
        let mut pid = process.pid;
        for _ in 0..DEPTH {
            if let Some((pane, _)) = roots.iter().find(|(_, root)| *root == pid) {
                process.pane = Some(*pane);
                break;
            }
            match parents.get(&pid) {
                Some(&parent) if parent != pid => pid = parent,
                _ => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "\
    1     0 root              20464 329:05.00 /sbin/launchd
  405     1 _windowserver    135136 4294:34.04 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer
59624 59613 ada               96784   1-02:03:04 /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper (Renderer)
    0     0 root                  0   0:00.00 kernel_task
not a process
";

    fn rows(text: &str) -> impl Iterator<Item = Row> {
        text.lines().filter_map(Row::parse)
    }

    #[test]
    fn a_line_of_ps_gives_the_program_its_owner_and_its_time() {
        let rows: Vec<_> = rows(PS).collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[1],
            Row {
                pid: 405,
                parent: 1,
                user: "_windowserver".into(),
                memory: 135_136 * 1024,
                partial: true,
                spent: 4294.0 * 60.0 + 34.04,
                name: "WindowServer".into(),
            }
        );
        // A path may hold spaces, and a time may run to days.
        assert_eq!(rows[2].name, "Google Chrome Helper (Renderer)");
        assert_eq!(seconds("1-02:03:04"), Some(93784.0));
        assert_eq!(rows[3].name, "kernel_task");
    }

    #[test]
    fn a_process_uses_the_processor_time_it_gained_between_two_readings() {
        let mut processes = Processes::default();
        let first = processes.take(rows(PS), Duration::from_secs(2));
        assert!(first.iter().all(|process| process.cpu == 0.0));

        // Two seconds on, the window server has used three more seconds, on
        // two processors, and a process has started.
        let later = PS.replace("4294:34.04", "4294:37.04")
            + "  900     1 ada                 512   0:00.50 /usr/bin/new\n";
        let second = processes.take(rows(&later), Duration::from_secs(2));
        assert_eq!(second[0].name, "WindowServer");
        assert_eq!(format!("{:.1}", second[0].cpu), "150.0");
        let new = second.iter().find(|process| process.pid == 900);
        assert_eq!(new.map(|process| process.cpu), Some(0.0));
        // The idle ones follow by the memory they hold.
        assert_eq!(second[1].name, "Google Chrome Helper (Renderer)");
    }

    #[test]
    fn a_process_belongs_to_the_pane_it_was_started_under() {
        let process = |pid, parent| Process {
            pid,
            parent,
            name: String::new(),
            user: String::new(),
            cpu: 0.0,
            memory: 0,
            partial: false,
            pane: None,
        };
        // A shell (10) in pane 1 started an agent (11), which started a tool
        // (12). Another pane holds 20, and 30 belongs to neither.
        let mut processes = [
            process(1, 0),
            process(10, 1),
            process(11, 10),
            process(12, 11),
            process(20, 1),
            process(30, 1),
        ];
        let (one, two) = (PaneId::new(1), PaneId::new(2));
        attribute(&mut processes, &[(one, 10), (two, 20)]);
        let panes: Vec<_> = processes.iter().map(|process| process.pane).collect();
        assert_eq!(panes, [None, Some(one), Some(one), Some(one), Some(two), None]);
    }
}
