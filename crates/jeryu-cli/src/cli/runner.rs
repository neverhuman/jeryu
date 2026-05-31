//! `jeryu runner` taxonomy: list, enroll, drain, and rotate build runners.
//!
//! Runners are jeryu runners. There are no runner pools and no foreign-CI
//! runner tokens; the default executor is native Rust. OCI is explicit.

use clap::{Subcommand, ValueEnum};

use crate::client::RunnerExecutor;

/// Runner executor backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RunnerExecutorArg {
    /// OCI container executor.
    Oci,
    /// Native host executor.
    Native,
}

impl From<RunnerExecutorArg> for RunnerExecutor {
    fn from(value: RunnerExecutorArg) -> Self {
        match value {
            RunnerExecutorArg::Oci => RunnerExecutor::Oci,
            RunnerExecutorArg::Native => RunnerExecutor::Native,
        }
    }
}

/// Runner command group.
#[derive(Debug, Subcommand)]
pub enum RunnerCommands {
    /// List registered runners and their executors.
    List,
    /// Enroll a node as a runner.
    Enroll {
        /// Node identifier to enroll.
        node: String,
        /// Executor backend for the enrolled runner.
        #[arg(long, value_enum, default_value_t = RunnerExecutorArg::Native)]
        executor: RunnerExecutorArg,
    },
    /// Drain a runner: stop accepting leases and await in-flight work.
    Drain {
        /// Runner identifier to drain.
        id: String,
    },
    /// Rotate a runner enrollment credential.
    Rotate {
        /// Runner identifier whose credential to rotate.
        id: String,
    },
}
