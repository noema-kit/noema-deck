//! The `monitor` command.

use anyhow::Result;
use noema_deck_core::monitor::Monitor;

/// Prints what the machine is doing, and the `count` busiest processes.
pub fn print(count: usize) -> Result<()> {
    let sample = Monitor::new()?.sample(&[])?;
    let (host, memory) = (&sample.host, &sample.memory);
    let (days, hours) = (host.uptime / 86400, host.uptime % 86400 / 3600);
    println!("{} · {} · {} · up {days}d {hours}h", host.name, host.cpu, host.system);
    let [one, five, fifteen] = sample.cpu.load;
    println!(
        "{:9} {:>5.1}%  load {one:.2} {five:.2} {fifteen:.2}, {} processors",
        "CPU",
        sample.cpu.usage,
        sample.cpu.cores.len()
    );
    println!(
        "{:9} {:>5.1}%  {} of {}, swap {} of {}",
        "Memory",
        percent(memory.used, memory.total),
        size(memory.used, 1024.0),
        size(memory.total, 1024.0),
        size(memory.swap_used, 1024.0),
        size(memory.swap_total, 1024.0)
    );
    for gpu in &sample.gpus {
        println!("{:9} {:>5.1}%  {}", "GPU", gpu.usage, gpu.name);
    }
    let rate = |bytes| size(bytes, 1000.0) + "/s";
    println!("{:9} read {}, written {}", "Disk", rate(sample.disk.read), rate(sample.disk.written));
    for volume in &sample.disk.volumes {
        let used = volume.total.saturating_sub(volume.available);
        println!(
            "{:9} {:>5.1}%  {} of {} on {}",
            "",
            percent(used, volume.total),
            size(used, 1000.0),
            size(volume.total, 1000.0),
            volume.name
        );
    }
    let network = &sample.network;
    println!("{:9} received {}, sent {}", "Network", rate(network.received), rate(network.sent));

    println!("\n{:>7}  {:16}  {:>6}  {:>8}  Process", "PID", "User", "CPU", "Memory");
    for process in sample.processes.iter().take(count) {
        println!(
            "{:>7}  {:16}  {:>5.1}%  {:>8}  {}",
            process.pid,
            process.user,
            process.cpu,
            size(process.memory, 1024.0),
            process.name
        );
    }
    Ok(())
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 { 0.0 } else { part as f64 / whole as f64 * 100.0 }
}

/// A count of bytes in at most four figures, such as `13.8 GB`. Memory is
/// counted in units of 1,024 and disks in units of 1,000, as macOS shows them.
fn size(bytes: u64, base: f64) -> String {
    let mut amount = bytes as f64;
    let mut unit = "B";
    for next in ["KB", "MB", "GB", "TB"] {
        if amount < base {
            break;
        }
        (amount, unit) = (amount / base, next);
    }
    if unit == "B" { format!("{amount} B") } else { format!("{amount:.1} {unit}") }
}
