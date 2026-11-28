//! The machine's activity: its processors, memory, graphics, disks, network
//! and processes, and how much of it each pane accounts for.
//!
//! Nothing is read until someone asks. A reading covers the time since the
//! one before, so a rate is the average over that time.

mod processes;

use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use serde::Serialize;
use sysinfo::{DiskRefreshKind, Disks, Gpus, Networks, System};

pub use self::processes::Process;
use self::processes::{Processes, attribute};
use crate::pane::PaneId;

/// A reading older than this is not built on: an average since then would
/// pass for what the machine is doing now.
const STALE: Duration = Duration::from_secs(10);
/// The shortest time a reading covers: how long a fresh start measures before
/// it answers, and how long a reading asked for too soon waits. Over less,
/// the hundredths of a second `ps` counts in make a process's share coarse.
const PRIME: Duration = Duration::from_millis(500);

/// What the machine is doing, as of one reading.
#[derive(Debug, Serialize)]
pub struct Sample {
    pub host: Host,
    pub cpu: Cpu,
    pub memory: Memory,
    /// The graphics processors that say how busy they are.
    pub gpus: Vec<Gpu>,
    pub disk: Disk,
    pub network: Network,
    /// Every process, busiest first.
    pub processes: Vec<Process>,
    /// What the processes of each pane add up to, for the panes that have any.
    pub panes: Vec<PaneLoad>,
}

#[derive(Debug, Serialize)]
pub struct Host {
    pub name: String,
    /// The system and its version, such as `macOS 26.5`.
    pub system: String,
    /// The processor's model.
    pub cpu: String,
    /// Seconds since the machine started.
    pub uptime: u64,
}

#[derive(Debug, Serialize)]
pub struct Cpu {
    /// How busy the processors are together, in percent.
    pub usage: f32,
    /// How busy each one is, in percent.
    pub cores: Vec<f32>,
    /// The load averaged over one, five and fifteen minutes.
    pub load: [f64; 3],
}

/// Memory in bytes. Used is what programs hold, what the system itself keeps
/// and what it has compressed, as Activity Monitor counts it. Files kept at
/// hand, which the system gives up when asked, do not count.
#[derive(Debug, Serialize)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Debug, Serialize)]
pub struct Gpu {
    pub name: String,
    /// How busy it is, in percent.
    pub usage: f32,
}

#[derive(Debug, Serialize)]
pub struct Disk {
    /// Bytes read and written each second.
    pub read: u64,
    pub written: u64,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Serialize)]
pub struct Volume {
    pub name: String,
    pub mount: String,
    /// Its size and the room left, in bytes.
    pub total: u64,
    pub available: u64,
}

#[derive(Debug, Serialize)]
pub struct Network {
    /// Bytes received and sent each second.
    pub received: u64,
    pub sent: u64,
    /// Bytes received and sent since the machine started.
    pub total_received: u64,
    pub total_sent: u64,
}

#[derive(Debug, Serialize)]
pub struct PaneLoad {
    pub pane: PaneId,
    /// The share of one processor its processes use together, in percent.
    pub cpu: f64,
    /// The memory they hold, in bytes.
    pub memory: u64,
    pub processes: usize,
}

pub struct Monitor {
    system: System,
    networks: Networks,
    disks: Disks,
    gpus: Gpus,
    processes: Processes,
    /// When the last reading was taken.
    read: Option<Instant>,
    /// Where the volumes of the reading before the last were mounted. A
    /// volume met since reports all its drive's traffic since the machine
    /// started as if it were new.
    mounted: Vec<PathBuf>,
}

impl Monitor {
    pub fn new() -> Result<Self> {
        Ok(Self {
            system: System::new()?,
            networks: Networks::new()?,
            disks: Disks::new()?,
            gpus: Gpus::new()?,
            processes: Processes::default(),
            read: None,
            mounted: Vec::new(),
        })
    }

    /// Reads the machine. `roots` names the process each open pane runs, so
    /// what was started under it counts for that pane. The first reading, and
    /// one long after the last, takes half a second to measure, and one asked
    /// for sooner than that after the last waits until then.
    pub fn sample(&mut self, roots: &[(PaneId, u32)]) -> Result<Sample> {
        match self.read.map(|read| read.elapsed()) {
            Some(since) if since <= STALE => thread::sleep(PRIME.saturating_sub(since)),
            _ => {
                self.read = None;
                self.refresh()?;
                thread::sleep(PRIME);
            }
        }
        let (elapsed, mut processes) = self.refresh()?;
        attribute(&mut processes, roots);
        // Bytes counted over the reading, as bytes each second.
        let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX).max(1);
        let rate = |bytes: u64| bytes.saturating_mul(1000) / millis;

        let load = System::load_average().unwrap_or_default();
        let mut network = Network { received: 0, sent: 0, total_received: 0, total_sent: 0 };
        // Only interfaces with an address of their own are counted. What goes
        // through a tunnel or the loopback is counted again, or never leaves.
        for (_, data) in &self.networks {
            if data.mac_address().is_unspecified() {
                continue;
            }
            network.received += rate(data.received());
            network.sent += rate(data.transmitted());
            network.total_received += data.total_received();
            network.total_sent += data.total_transmitted();
        }
        let mut disk = Disk { read: 0, written: 0, volumes: Vec::new() };
        for volume in &self.disks {
            // Every volume of a drive reports the drive's traffic, so the
            // busiest one speaks for the machine.
            if self.mounted.iter().any(|mount| mount == volume.mount_point()) {
                let usage = volume.usage();
                disk.read = disk.read.max(rate(usage.read_bytes));
                disk.written = disk.written.max(rate(usage.written_bytes));
            }
            let name = volume.name().to_string_lossy().into_owned();
            let total = volume.total_space();
            // macOS mounts a system volume in two parts, which share a name.
            if total > 0
                && !disk.volumes.iter().any(|seen| seen.name == name && seen.total == total)
            {
                let mount = volume.mount_point().to_string_lossy().into_owned();
                disk.volumes.push(Volume {
                    name,
                    mount,
                    total,
                    available: volume.available_space(),
                });
            }
        }
        let gpus = self.gpus.iter().filter_map(|gpu| {
            let name = gpu.model().or_else(|| gpu.vendor()).unwrap_or("Graphics");
            Some(Gpu { name: name.into(), usage: gpu.usage()? })
        });

        Ok(Sample {
            host: Host {
                name: System::host_name().unwrap_or_default(),
                system: System::long_os_version().unwrap_or_default(),
                cpu: self.system.cpus().first().map_or("", |cpu| cpu.brand().trim()).into(),
                uptime: System::uptime().unwrap_or_default(),
            },
            cpu: Cpu {
                usage: self.system.global_cpu_usage(),
                cores: self.system.cpus().iter().map(sysinfo::Cpu::usage).collect(),
                load: [load.one, load.five, load.fifteen],
            },
            memory: Memory {
                total: self.system.total_memory(),
                used: self.system.used_memory(),
                swap_total: self.system.total_swap(),
                swap_used: self.system.used_swap(),
            },
            gpus: gpus.collect(),
            disk,
            network,
            panes: loads(&processes),
            processes,
        })
    }

    /// Reads everything again. Answers with the time since the last reading
    /// and the processes.
    fn refresh(&mut self) -> Result<(Duration, Vec<Process>)> {
        let now = Instant::now();
        let elapsed = self.read.replace(now).map_or(Duration::ZERO, |read| now - read);
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        self.mounted = self.disks.iter().map(|volume| volume.mount_point().into()).collect();
        self.disks
            .refresh_specifics(true, DiskRefreshKind::nothing().with_storage().with_io_usage());
        self.gpus.refresh(true);
        Ok((elapsed, self.processes.read()?))
    }
}

/// What each pane's processes add up to, in the panes' order.
fn loads(processes: &[Process]) -> Vec<PaneLoad> {
    let mut loads = Vec::<PaneLoad>::new();
    for process in processes {
        let Some(pane) = process.pane else { continue };
        let at = loads.iter().position(|load| load.pane == pane).unwrap_or_else(|| {
            loads.push(PaneLoad { pane, cpu: 0.0, memory: 0, processes: 0 });
            loads.len() - 1
        });
        let load = &mut loads[at];
        load.cpu += process.cpu;
        load.memory += process.memory;
        load.processes += 1;
    }
    loads.sort_by_key(|load| load.pane);
    loads
}

#[cfg(test)]
mod tests {
    use std::process;

    use super::*;

    #[test]
    fn a_reading_covers_the_machine_and_counts_a_panes_processes() -> Result<()> {
        let pane = PaneId::new(1);
        let sample = Monitor::new()?.sample(&[(pane, process::id())])?;
        assert!(!sample.cpu.cores.is_empty());
        assert!(sample.memory.total > 0 && sample.memory.used <= sample.memory.total);
        assert!(!sample.disk.volumes.is_empty());

        // This test stands in for a pane's process, so it counts for the pane.
        let own = sample.processes.iter().find(|process| process.pid == process::id());
        assert_eq!(own.and_then(|process| process.pane), Some(pane));
        let [load] = &sample.panes[..] else { panic!("the pane has no load: {:?}", sample.panes) };
        assert!(load.pane == pane && load.processes >= 1);
        // The kernel tells the whole footprint of one's own process.
        assert!(own.is_some_and(|process| !process.partial && process.memory > 0));
        assert!(load.memory > 0);
        Ok(())
    }
}
