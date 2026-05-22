use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum BugAttemptCommands {
    Start {
        bug_id: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        sandbox_path: Option<PathBuf>,
    },
    Fail {
        bug_id: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        ci_evidence: Option<String>,
    },
    Complete {
        bug_id: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        pr_url: Option<String>,
        #[arg(long)]
        head_sha: Option<String>,
        #[arg(long)]
        notes: Option<String>,
    },
}
