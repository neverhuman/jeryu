use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum PolicyCommands {
    /// Audit configured policy against a target control plane.
    Audit {
        #[arg(long, default_value = "local-gitlab")]
        target: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
