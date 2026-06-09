#![doc = "Load-aware CI/build worker authority for jeryu."]
#![doc = ""]
#![doc = "A runaway host-CI run (`WORKERS=40`, `pytest -n 40`, three repos in"]
#![doc = "parallel with no resource bound) once wedged a 125 GB / 128-core box: load"]
#![doc = "841, swap pinned at 100%. The lesson is that the worker count must be a"]
#![doc = "decision jeryu owns from live system pressure, not a number a repo or user"]
#![doc = "gets to demand. This crate is that authority."]
#![doc = ""]
#![doc = "The crate is split deliberately:"]
#![doc = "* [`decide_jobs`] is a PURE, side-effect-free function — it is the single"]
#![doc = "  source of truth for the worker count and is exhaustively unit tested with"]
#![doc = "  no `/proc` access at all."]
#![doc = "* [`SystemLoad::probe`] is the only part that touches `/proc`; it reads live"]
#![doc = "  pressure and, on any read or parse trouble, returns conservative defaults"]
#![doc = "  so the authority resolves to few workers and never panics."]
#![doc = ""]
#![doc = "jeryu always wins on the upper bound: a request above the computed safe cap"]
#![doc = "is clamped down to the cap. A request below the cap is honored."]

/// A snapshot of live system pressure used to size the worker pool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemLoad {
    /// Number of logical CPUs.
    pub ncpu: u32,
    /// Total physical memory in bytes.
    pub total_mem_bytes: u64,
    /// Memory the kernel estimates is available for new work, in bytes.
    pub available_mem_bytes: u64,
    /// Total configured swap in bytes.
    pub swap_total_bytes: u64,
    /// Swap in use in bytes (the actual thrash signal).
    pub swap_used_bytes: u64,
    /// The 1-minute load average.
    pub load1: f64,
}

/// One mebibyte in bytes.
const MIB: u64 = 1024 * 1024;
/// One gibibyte in bytes.
const GIB: u64 = 1024 * MIB;

impl SystemLoad {
    /// Read live system pressure from `/proc`.
    ///
    /// Reads `MemTotal`, `MemAvailable`, `SwapTotal` and `SwapFree` from
    /// `/proc/meminfo`, the first field of `/proc/loadavg`, and the CPU count
    /// from [`std::thread::available_parallelism`].
    ///
    /// This is the only function in the crate that performs I/O. It is written
    /// to fail safe: if any file is missing or any field cannot be parsed it
    /// resolves that field to a conservative default (one CPU, a small amount
    /// of available memory, a high-but-finite load) so that [`decide_jobs`]
    /// sizes the pool down rather than up. It never panics.
    #[must_use]
    pub fn probe() -> SystemLoad {
        let ncpu = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1) as u32;

        let mut total_mem_bytes = 0u64;
        let mut available_mem_bytes = 0u64;
        let mut swap_total_bytes = 0u64;
        let mut swap_free_bytes = 0u64;
        let mut saw_available = false;

        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                let Some((key, rest)) = line.split_once(':') else {
                    continue;
                };
                let Some(kb) = parse_meminfo_kb(rest) else {
                    continue;
                };
                let bytes = kb.saturating_mul(1024);
                match key {
                    "MemTotal" => total_mem_bytes = bytes,
                    "MemAvailable" => {
                        available_mem_bytes = bytes;
                        saw_available = true;
                    }
                    "SwapTotal" => swap_total_bytes = bytes,
                    "SwapFree" => swap_free_bytes = bytes,
                    _ => {}
                }
            }
        }

        // A kernel old enough to lack MemAvailable, or an unreadable file,
        // leaves the estimate at the conservative floor so the pool stays small.
        if !saw_available || available_mem_bytes == 0 {
            available_mem_bytes = 512 * MIB;
        }
        let swap_used_bytes = swap_total_bytes.saturating_sub(swap_free_bytes);

        let load1 = std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|s| s.split_whitespace().next().map(str::to_owned))
            .and_then(|f| f.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(f64::from(ncpu));

        SystemLoad {
            ncpu,
            total_mem_bytes,
            available_mem_bytes,
            swap_total_bytes,
            swap_used_bytes,
            load1,
        }
    }
}

/// Parse the numeric kibibyte value out of a `/proc/meminfo` value field
/// such as `   16384256 kB`.
fn parse_meminfo_kb(rest: &str) -> Option<u64> {
    rest.split_whitespace().next()?.parse::<u64>().ok()
}

/// Which constraint determined the final worker count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// Available memory divided by the per-job memory budget.
    Memory,
    /// CPU count times the configured CPU fraction.
    Cpu,
    /// The 1-minute load average is high relative to CPU count.
    Load,
    /// The hard per-repo safety ceiling.
    Ceiling,
    /// The minimum worker count.
    Floor,
    /// Swap is in active use; the pool is clamped hard against thrash.
    Swap,
    /// The user/repo request, honored because it sits below the safe cap.
    UserRequest,
}

impl Bound {
    /// A short stable label for logs and receipts.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Bound::Memory => "memory",
            Bound::Cpu => "cpu",
            Bound::Load => "load",
            Bound::Ceiling => "ceiling",
            Bound::Floor => "floor",
            Bound::Swap => "swap",
            Bound::UserRequest => "user-request",
        }
    }
}

/// The authority's verdict: how many workers may run, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsDecision {
    /// The number of workers permitted. Always at least the configured floor.
    pub jobs: u32,
    /// The constraint that determined `jobs`.
    pub binding: Bound,
    /// A short human-readable explanation naming the bound and the numbers.
    pub reason: String,
}

/// Tunables for [`decide_jobs`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GovernorConfig {
    /// Memory budgeted per worker, in bytes.
    pub per_job_mem_bytes: u64,
    /// Fraction of CPUs a single repo may claim.
    pub cpu_fraction: f64,
    /// The hard per-repo worker ceiling. No single repo may exceed this
    /// regardless of cores or request — the central safety invariant.
    pub hard_ceiling: u32,
    /// The minimum worker count; the pool never resolves below this or to zero.
    pub floor: u32,
    /// Load-average-per-CPU above which the pool is halved.
    pub load_high_ratio: f64,
    /// Swap-in-use above which the pool is clamped hard against thrash.
    pub swap_used_high_bytes: u64,
}

impl Default for GovernorConfig {
    fn default() -> Self {
        GovernorConfig {
            per_job_mem_bytes: 2 * GIB,
            cpu_fraction: 0.25,
            hard_ceiling: 16,
            floor: 1,
            load_high_ratio: 0.7,
            swap_used_high_bytes: GIB,
        }
    }
}

/// The hard clamp imposed when swap is actively in use. Swap thrash was the
/// actual death of the wedged box, so this is deliberately aggressive.
const SWAP_THRASH_CAP: u32 = 2;

/// Decide how many CI/build workers a single repo may run.
///
/// This is the pure authority — given a [`SystemLoad`] snapshot, an optional
/// caller `request`, and a [`GovernorConfig`], it returns the permitted worker
/// count with no side effects.
///
/// The safe cap is the smaller of a memory budget and a CPU-fraction budget,
/// held under a hard ceiling, then throttled for high load and clamped hard
/// when swap is in use, and finally lifted to the floor. jeryu always wins on
/// the upper bound: a `request` above the safe cap is clamped down to the cap
/// (the binding stays the resource bound and the reason records the clamp); a
/// `request` at or below the cap is honored; `None` resolves to the cap.
#[must_use]
pub fn decide_jobs(load: &SystemLoad, request: Option<u32>, cfg: &GovernorConfig) -> JobsDecision {
    let ncpu = load.ncpu.max(1);
    let per_job = cfg.per_job_mem_bytes.max(1);

    let mem_cap = u32::try_from(load.available_mem_bytes / per_job)
        .unwrap_or(u32::MAX)
        .max(1);
    let cpu_cap = ((f64::from(ncpu) * cfg.cpu_fraction).floor() as i64)
        .max(1)
        .min(i64::from(u32::MAX)) as u32;

    // Smaller of the two resource budgets sets the base and its binding.
    let (mut base, mut binding) = if mem_cap <= cpu_cap {
        (mem_cap, Bound::Memory)
    } else {
        (cpu_cap, Bound::Cpu)
    };

    // Hard ceiling: no single repo may exceed it, whatever the resources say.
    if base > cfg.hard_ceiling {
        base = cfg.hard_ceiling;
        binding = Bound::Ceiling;
    }

    // Load throttle: a box already above the load-per-CPU line gets the pool
    // halved so a new repo does not pile onto the run queue.
    if f64::from(ncpu) > 0.0 && load.load1 / f64::from(ncpu) > cfg.load_high_ratio {
        base = (base / 2).max(1);
        binding = Bound::Load;
    }

    // Swap clamp: any active swap means the box is already paging; squeeze the
    // pool to a hard minimum. This is the guard that the wedged box lacked.
    if load.swap_used_bytes > cfg.swap_used_high_bytes {
        base = base.min(SWAP_THRASH_CAP);
        binding = Bound::Swap;
    }

    // Floor: never zero, never below the configured minimum.
    if base < cfg.floor {
        base = cfg.floor;
        binding = Bound::Floor;
    }

    let cap = base;

    match request {
        None => JobsDecision {
            jobs: cap,
            binding,
            reason: cap_reason(load, cfg, binding, cap, ncpu, mem_cap),
        },
        Some(req) if req > cap => {
            let mut reason = cap_reason(load, cfg, binding, cap, ncpu, mem_cap);
            reason.push_str(&format!("; clamped request {req}->{cap}"));
            JobsDecision {
                jobs: cap,
                binding,
                reason,
            }
        }
        Some(req) => {
            let honored = req.max(cfg.floor);
            JobsDecision {
                jobs: honored,
                binding: Bound::UserRequest,
                reason: format!("user-request honored: {honored} <= cap {cap}"),
            }
        }
    }
}

/// Render the human-readable reason for a cap set by `binding`.
fn cap_reason(
    load: &SystemLoad,
    cfg: &GovernorConfig,
    binding: Bound,
    cap: u32,
    ncpu: u32,
    mem_cap: u32,
) -> String {
    let avail_gib = load.available_mem_bytes as f64 / GIB as f64;
    let per_job_gib = cfg.per_job_mem_bytes as f64 / GIB as f64;
    let swap_gib = load.swap_used_bytes as f64 / GIB as f64;
    let swap_high_gib = cfg.swap_used_high_bytes as f64 / GIB as f64;
    match binding {
        Bound::Memory => {
            format!("memory-bound: {cap} = {avail_gib:.0}GiB avail / {per_job_gib:.0}GiB per job")
        }
        Bound::Cpu => format!(
            "cpu-bound: {cap} = floor({ncpu} cpu * {:.2})",
            cfg.cpu_fraction
        ),
        Bound::Ceiling => {
            format!("ceiling-bound: {cap} = hard ceiling (resources/request would allow more)")
        }
        Bound::Load => format!(
            "load-bound: {cap}; load1 {:.2} / {ncpu} cpu > {:.2} so the pool was halved",
            load.load1, cfg.load_high_ratio
        ),
        Bound::Swap => format!(
            "swap-bound: {cap}; swap in use {swap_gib:.1}GiB > {swap_high_gib:.1}GiB so the pool was clamped"
        ),
        Bound::Floor => format!("floor-bound: {cap} = configured minimum"),
        // The resource budgets feed mem_cap; surface it when nothing else binds.
        Bound::UserRequest => format!("cap {cap} (mem budget {mem_cap})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A roomy 128-core, 125 GiB box with negligible load and no swap.
    fn big_idle_box() -> SystemLoad {
        SystemLoad {
            ncpu: 128,
            total_mem_bytes: 125 * GIB,
            available_mem_bytes: 120 * GIB,
            swap_total_bytes: 64 * GIB,
            swap_used_bytes: 0,
            load1: 2.0,
        }
    }

    #[test]
    fn idle_big_box_with_no_request_is_capped_at_hard_ceiling() {
        // Proves a 128-core box never gets 40+ workers: cpu_cap would be 32,
        // mem_cap far higher, so the hard ceiling (16) is what binds.
        let d = decide_jobs(&big_idle_box(), None, &GovernorConfig::default());
        assert_eq!(d.jobs, 16);
        assert_eq!(d.binding, Bound::Ceiling);
    }

    #[test]
    fn scarce_memory_makes_the_pool_memory_bound() {
        // Only 4 GiB available against a 2 GiB per-job budget => 2 workers,
        // and memory is the binding constraint even on a many-core box.
        let load = SystemLoad {
            available_mem_bytes: 4 * GIB,
            ..big_idle_box()
        };
        let d = decide_jobs(&load, None, &GovernorConfig::default());
        assert_eq!(d.jobs, 2);
        assert_eq!(d.binding, Bound::Memory);
    }

    #[test]
    fn request_of_40_on_a_big_box_is_clamped_to_the_ceiling() {
        // THE crash scenario: a repo demands 40 workers; jeryu clamps it to the
        // hard ceiling and records the clamp in the reason.
        let d = decide_jobs(&big_idle_box(), Some(40), &GovernorConfig::default());
        assert_eq!(d.jobs, 16);
        assert_eq!(d.binding, Bound::Ceiling);
        assert!(
            d.reason.contains("clamped request 40->16"),
            "reason was: {}",
            d.reason
        );
    }

    #[test]
    fn request_below_the_cap_is_honored() {
        // An 8-wide cap with a request of 4 honors the smaller request.
        let cfg = GovernorConfig {
            hard_ceiling: 8,
            ..GovernorConfig::default()
        };
        // 32 cores -> cpu_cap 8; ample memory keeps the cap at 8.
        let load = SystemLoad {
            ncpu: 32,
            ..big_idle_box()
        };
        let d = decide_jobs(&load, Some(4), &cfg);
        assert_eq!(d.jobs, 4);
        assert_eq!(d.binding, Bound::UserRequest);
    }

    #[test]
    fn high_load_halves_the_pool() {
        // load1/ncpu above the 0.7 ratio halves the base and binds on load.
        let load = SystemLoad {
            ncpu: 8,
            load1: 7.0, // 7/8 = 0.875 > 0.7
            ..big_idle_box()
        };
        // cpu_cap = floor(8 * 0.25) = 2; halved => 1.
        let d = decide_jobs(&load, None, &GovernorConfig::default());
        assert_eq!(d.binding, Bound::Load);
        assert_eq!(d.jobs, 1);
    }

    #[test]
    fn high_load_halves_a_larger_base() {
        // A larger base makes the halving visible as a value, not just a floor.
        let load = SystemLoad {
            ncpu: 64,
            load1: 60.0, // 60/64 = 0.9375 > 0.7
            ..big_idle_box()
        };
        // cpu_cap = 16, capped at ceiling 16, halved => 8.
        let d = decide_jobs(&load, None, &GovernorConfig::default());
        assert_eq!(d.binding, Bound::Load);
        assert_eq!(d.jobs, 8);
    }

    #[test]
    fn active_swap_clamps_the_pool_hard() {
        // Any meaningful swap-in-use squeezes the pool to at most 2 workers,
        // binding on swap — the guard the wedged box never had.
        let load = SystemLoad {
            swap_used_bytes: 8 * GIB,
            ..big_idle_box()
        };
        let d = decide_jobs(&load, None, &GovernorConfig::default());
        assert!(d.jobs <= 2, "expected <=2 got {}", d.jobs);
        assert_eq!(d.binding, Bound::Swap);
    }

    #[test]
    fn swap_clamp_overrides_a_user_request() {
        // Even a user request cannot lift the pool above the swap clamp.
        let load = SystemLoad {
            swap_used_bytes: 8 * GIB,
            ..big_idle_box()
        };
        let d = decide_jobs(&load, Some(40), &GovernorConfig::default());
        assert!(d.jobs <= 2, "expected <=2 got {}", d.jobs);
        assert_eq!(d.binding, Bound::Swap);
    }

    #[test]
    fn floor_is_respected_and_never_zero() {
        // A starved single-core box still gets exactly the floor, never zero.
        let load = SystemLoad {
            ncpu: 1,
            total_mem_bytes: 512 * MIB,
            available_mem_bytes: 256 * MIB,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            load1: 0.1,
        };
        let d = decide_jobs(&load, None, &GovernorConfig::default());
        assert_eq!(d.jobs, GovernorConfig::default().floor);
        assert_eq!(d.jobs, 1);
        assert!(d.jobs > 0);
    }

    #[test]
    fn probe_reports_at_least_one_cpu_and_does_not_panic() {
        // probe() reads /proc; assert only the cheap, stable invariants.
        let load = SystemLoad::probe();
        assert!(load.ncpu >= 1);
        assert!(load.available_mem_bytes > 0);
        assert!(load.load1.is_finite());
    }

    #[test]
    fn probe_output_yields_a_sane_decision() {
        // A live probe must still resolve to a bounded, floor-respecting count.
        let cfg = GovernorConfig::default();
        let d = decide_jobs(&SystemLoad::probe(), None, &cfg);
        assert!(d.jobs >= cfg.floor);
        assert!(d.jobs <= cfg.hard_ceiling);
    }
}
