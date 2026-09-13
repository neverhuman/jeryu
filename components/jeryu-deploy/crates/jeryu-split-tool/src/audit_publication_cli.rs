//! Thin local interface. Context files remain unqualified data.
use super::*;

#[derive(Debug, clap::Args)]
pub(crate) struct Arguments {
    #[arg(long)]
    context: PathBuf,
    #[arg(long)]
    receipt: PathBuf,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    candidate_policy: PathBuf,
    #[arg(long)]
    governing_policy: PathBuf,
    #[arg(long)]
    dependency_lock: PathBuf,
    #[arg(long)]
    execution_config: PathBuf,
    #[arg(long)]
    auditor_receipt: PathBuf,
    #[arg(long)]
    renderer_output: Option<PathBuf>,
    /// Existing physical owner-only directory outside the source checkout.
    #[arg(long)]
    output_root: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalContext {
    schema_version: String,
    identity: JsonObject<Identity>,
    command_exit: Option<i32>,
    report_sha256: Option<String>,
    renderer: Option<JsonObject<RendererObservation>>,
}

pub(crate) fn run(root: &Path, args: Arguments) -> Result<()> {
    let JsonObject(context): JsonObject<LocalContext> =
        serde_json::from_slice(&storage::read_file(&args.context, MAX_ARTIFACT)?)?;
    ensure!(
        context.schema_version == "jeryu.audit-package-local-context/v1",
        "unsupported local preparation context"
    );
    let expected = ExpectedObservation {
        identity: context.identity.0,
        command_exit: context.command_exit,
        report_sha256: context.report_sha256,
        renderer: context.renderer.map(|renderer| renderer.0),
    };
    let receipt = storage::read_file(&args.receipt, MAX_ARTIFACT)?;
    // A missing requested report is retained as an ERROR preparation. No previous
    // report or SVG is substituted when artifact custody/read admission fails.
    let report = args
        .report
        .map(|path| storage::read_file(&path, MAX_ARTIFACT))
        .transpose();
    let renderer = args
        .renderer_output
        .map(|path| storage::read_file(&path, MAX_SVG))
        .transpose();
    let candidate = storage::read_file(&args.candidate_policy, MAX_ARTIFACT)?;
    let governing = storage::read_file(&args.governing_policy, MAX_ARTIFACT)?;
    let lock = storage::read_file(&args.dependency_lock, MAX_ARTIFACT)?;
    let config = storage::read_file(&args.execution_config, MAX_ARTIFACT)?;
    let auditor_receipt = storage::read_file(&args.auditor_receipt, MAX_ARTIFACT)?;
    let bundle = prepare(
        root,
        &expected,
        Inputs {
            receipt: &receipt,
            report: report.as_ref().ok().and_then(|report| report.as_deref()),
            candidate_policy: &candidate,
            governing_policy: &governing,
            dependency_lock: &lock,
            execution_config: &config,
            auditor_receipt: &auditor_receipt,
            renderer_output: renderer
                .as_ref()
                .ok()
                .and_then(|renderer| renderer.as_deref()),
            renderer_unavailable: renderer.is_err(),
        },
    )?;
    let stored = create_once(&args.output_root, &bundle)?;
    println!(
        "{}",
        crate::canonical_json::pretty(json!({"storage":stored,"preparation":bundle.metadata(),
        "report_file_unavailable":report.is_err(),"renderer_file_unavailable":renderer.is_err()}))?
    );
    // Successful private storage is never the required publication check.
    bundle.require_publication_admission()
}
