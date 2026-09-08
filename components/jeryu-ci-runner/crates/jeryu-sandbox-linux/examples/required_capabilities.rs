//! Admission probe for the privileged CI lane. Missing capabilities are errors.

use jeryu_sandbox_linux::capability::SandboxCapabilities;

fn main() -> std::process::ExitCode {
    let caps = SandboxCapabilities::probe();
    println!("{}", caps.summary());
    if caps.user_namespace
        && caps.mount_namespace
        && caps.pid_namespace
        && caps.landlock_abi.is_some()
        && caps.seccomp_bpf
        && caps.cgroup_v2_subtree.is_some()
        && caps.no_new_privs
    {
        std::process::ExitCode::SUCCESS
    } else {
        eprintln!(
            "required sandbox capabilities are missing; privileged qualification cannot pass"
        );
        std::process::ExitCode::FAILURE
    }
}
