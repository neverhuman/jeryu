pub async fn gc_orphaned_workers() -> u64 {
    use std::fs;
    let Ok(proc_dir) = fs::read_dir("/proc") else {
        return 0;
    };
    let mut killed = 0u64;
    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let status_path = format!("/proc/{pid}/status");
        let Ok(status) = fs::read_to_string(&status_path) else {
            continue;
        };
        let ppid: u32 = status
            .lines()
            .find(|l| l.starts_with("PPid:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(u32::MAX);
        if ppid != 1 {
            continue;
        }
        let cmdline_path = format!("/proc/{pid}/cmdline");
        let Ok(cmdline) = fs::read_to_string(&cmdline_path) else {
            continue;
        };
        let cmd = cmdline.replace('\0', " ");
        if !cmd.contains("forkserver") && !cmd.contains("local_run_mimo") {
            continue;
        }
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
        killed += 1;
    }
    killed
}

pub fn mem_available_gb() -> f64 {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();

    meminfo
        .lines()
        .find(|l| l.starts_with("MemAvailable:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|kb| kb / 1024.0 / 1024.0)
        .unwrap_or(f64::MAX)
}
