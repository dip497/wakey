//! System sensor — captures battery, CPU usage, and RAM usage.
//!
//! Reads from Linux /proc and /sys filesystems directly.
//! No external dependencies, lightweight and fast.

use std::fs;
use std::path::Path;
use tracing::{debug, warn};

/// System vitals snapshot.
#[derive(Debug, Clone, Copy)]
pub struct SystemVitals {
    pub battery_percent: Option<u8>,
    pub cpu_usage: f32,
    pub ram_usage_mb: u64,
}

/// Get current system vitals.
///
/// Reads from:
/// - `/sys/class/power_supply/` for battery
/// - `/proc/stat` for CPU usage
/// - `/proc/meminfo` for RAM usage
pub fn get_system_vitals() -> SystemVitals {
    SystemVitals {
        battery_percent: get_battery_percent(),
        cpu_usage: get_cpu_usage(),
        ram_usage_mb: get_ram_usage_mb(),
    }
}

/// Get battery percentage from /sys/class/power_supply.
///
/// Looks for BAT0, BAT1, or any battery device.
fn get_battery_percent() -> Option<u8> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    if !power_supply_path.exists() {
        debug!("Power supply path not found");
        return None;
    }

    // Find any battery device
    for entry in fs::read_dir(power_supply_path).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Check if it's a battery (not AC adapter)
        let type_path = power_supply_path.join(&name).join("type");
        if let Ok(type_str) = fs::read_to_string(&type_path)
            && type_str.trim() != "Battery"
        {
            continue;
        }

        // Read capacity
        let capacity_path = power_supply_path.join(&name).join("capacity");
        if let Ok(capacity_str) = fs::read_to_string(&capacity_path) {
            let capacity = capacity_str.trim().parse::<u8>().ok()?;
            debug!(battery = capacity, "Battery level detected");
            return Some(capacity);
        }
    }

    warn!("No battery device found");
    None
}

/// Get CPU usage percentage.
///
/// Calculates usage from /proc/stat between two samples.
/// First call returns 0.0 and initializes the baseline.
static mut PREV_CPU_STATS: Option<CpuStats> = None;

fn get_cpu_usage() -> f32 {
    // SAFETY: Single-threaded access during initialization.
    // In a multi-threaded context, this would need proper synchronization.
    let prev = unsafe { PREV_CPU_STATS };

    let current = read_cpu_stats();

    if prev.is_none() {
        // First call, initialize baseline
        unsafe { PREV_CPU_STATS = Some(current) };
        return 0.0;
    }

    let prev = unsafe { PREV_CPU_STATS.unwrap() };
    let usage = calculate_cpu_usage(prev, current);

    // Update baseline for next call
    unsafe { PREV_CPU_STATS = Some(current) };

    usage
}

#[derive(Debug, Clone, Copy)]
struct CpuStats {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
}

fn read_cpu_stats() -> CpuStats {
    let stat_path = Path::new("/proc/stat");

    match fs::read_to_string(stat_path) {
        Ok(content) => parse_cpu_stat(&content),
        Err(e) => {
            warn!(error = %e, "Failed to read /proc/stat");
            CpuStats {
                user: 0,
                nice: 0,
                system: 0,
                idle: 0,
                iowait: 0,
                irq: 0,
                softirq: 0,
            }
        }
    }
}

/// Parse the first line of /proc/stat (aggregate CPU).
///
/// Format: `cpu  user nice system idle iowait irq softirq steal guest guest_nice`
fn parse_cpu_stat(content: &str) -> CpuStats {
    let first_line = content.lines().next().unwrap_or("cpu 0 0 0 0 0 0 0");

    let parts: Vec<u64> = first_line
        .split_whitespace()
        .skip(1) // Skip "cpu"
        .filter_map(|s| s.parse().ok())
        .collect();

    CpuStats {
        user: parts.first().copied().unwrap_or(0),
        nice: parts.get(1).copied().unwrap_or(0),
        system: parts.get(2).copied().unwrap_or(0),
        idle: parts.get(3).copied().unwrap_or(0),
        iowait: parts.get(4).copied().unwrap_or(0),
        irq: parts.get(5).copied().unwrap_or(0),
        softirq: parts.get(6).copied().unwrap_or(0),
    }
}

fn calculate_cpu_usage(prev: CpuStats, current: CpuStats) -> f32 {
    let prev_idle = prev.idle + prev.iowait;
    let current_idle = current.idle + current.iowait;

    let prev_total =
        prev.user + prev.nice + prev.system + prev.idle + prev.iowait + prev.irq + prev.softirq;
    let current_total = current.user
        + current.nice
        + current.system
        + current.idle
        + current.iowait
        + current.irq
        + current.softirq;

    let idle_delta = current_idle - prev_idle;
    let total_delta = current_total - prev_total;

    if total_delta == 0 {
        return 0.0;
    }

    let usage = ((total_delta - idle_delta) as f32 / total_delta as f32) * 100.0;
    usage.clamp(0.0, 100.0)
}

/// Get RAM usage in megabytes.
///
/// Reads MemTotal and MemAvailable from /proc/meminfo.
fn get_ram_usage_mb() -> u64 {
    let meminfo_path = Path::new("/proc/meminfo");

    let content = match fs::read_to_string(meminfo_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to read /proc/meminfo");
            return 0;
        }
    };

    let mem_total_kb = parse_meminfo_value(&content, "MemTotal");
    let mem_available_kb = parse_meminfo_value(&content, "MemAvailable");

    let used_kb = mem_total_kb.saturating_sub(mem_available_kb);
    used_kb / 1024 // Convert to MB
}

/// Parse a value from /proc/meminfo.
///
/// Format: `MemTotal:       16384000 kB`
fn parse_meminfo_value(content: &str, key: &str) -> u64 {
    for line in content.lines() {
        if line.starts_with(key) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse().unwrap_or(0);
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_stat() {
        let content = "cpu  100 20 30 400 10 5 5\ncpu0 50 10 15 200 5 2 2";
        let stats = parse_cpu_stat(content);

        assert_eq!(stats.user, 100);
        assert_eq!(stats.nice, 20);
        assert_eq!(stats.system, 30);
        assert_eq!(stats.idle, 400);
        assert_eq!(stats.iowait, 10);
        assert_eq!(stats.irq, 5);
        assert_eq!(stats.softirq, 5);
    }

    #[test]
    fn test_calculate_cpu_usage() {
        // CPU stats are cumulative jiffies, so values always increase over time
        // prev_idle must be less than current_idle

        let prev = CpuStats {
            user: 1000,
            nice: 0,
            system: 500,
            idle: 8500,
            iowait: 100,
            irq: 0,
            softirq: 0,
        };

        let current = CpuStats {
            user: 1100,
            nice: 0,
            system: 600,
            idle: 8700,
            iowait: 120,
            irq: 0,
            softirq: 0,
        };

        // prev_idle = 8500 + 100 = 8600
        // current_idle = 8700 + 120 = 8820
        // idle_delta = 8820 - 8600 = 220
        // prev_total = 1000+0+500+8500+100+0+0 = 10100
        // current_total = 1100+0+600+8700+120+0+0 = 10520
        // total_delta = 10520 - 10100 = 420
        // usage = (420 - 220) / 420 * 100 = 200/420 * 100 = 47.6%

        let usage = calculate_cpu_usage(prev, current);
        assert!((usage - 47.619047).abs() < 0.1);

        // Edge case: zero delta
        let usage_zero = calculate_cpu_usage(prev, prev);
        assert_eq!(usage_zero, 0.0);
    }

    #[test]
    fn test_parse_meminfo_value() {
        let content = "MemTotal:       16384000 kB\nMemAvailable:    8000000 kB\n";
        assert_eq!(parse_meminfo_value(content, "MemTotal"), 16384000);
        assert_eq!(parse_meminfo_value(content, "MemAvailable"), 8000000);
    }
}
