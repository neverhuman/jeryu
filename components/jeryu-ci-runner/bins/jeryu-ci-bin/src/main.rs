use jeryu_artifact_metadata::plan_artifacts;
use jeryu_cache_policy::plan_cache_for_job;
use jeryu_ci_compiler::{CiKind, CompileContext, Compiler};
use jeryu_ci_ir::{RunnerClass, TrustTier};
use jeryu_ci_scheduler::deterministic_schedule;
use std::env;
use std::fs;
use std::process::ExitCode;
use std::str::FromStr;

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("jeryu-ci-bin: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<String, String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(help());
    }
    let command = args[0].as_str();
    let options = Options::parse(&args[1..])?;
    let input = fs::read_to_string(&options.file)
        .map_err(|err| format!("cannot read {}: {err}", options.file))?;
    let mut context = CompileContext::new(options.repo, options.commit);
    context.trust_tier = options.trust_tier;
    context.default_runner = options.default_runner;
    let pipeline =
        Compiler::compile(&input, options.kind, context).map_err(|err| err.to_string())?;

    match command {
        "compile" => Ok(format!(
            "IR_HASH={}\nPIPELINE_ID={}\n{}",
            pipeline.ir_hash(),
            pipeline.id,
            pipeline.canonical()
        )),
        "schedule" => {
            let schedule = deterministic_schedule(&pipeline).map_err(|err| err.to_string())?;
            let mut out = String::new();
            out.push_str(&format!("SCHEDULE_HASH={}\n", schedule.schedule_hash));
            for round in schedule.rounds {
                out.push_str(&format!(
                    "round {}: {}\n",
                    round.index,
                    round.jobs.join(",")
                ));
            }
            Ok(out)
        }
        "explain" => explain(&pipeline),
        other => Err(format!("unknown command: {other}\n{}", help())),
    }
}

#[derive(Clone, Debug)]
struct Options {
    kind: CiKind,
    file: String,
    repo: String,
    commit: String,
    trust_tier: TrustTier,
    default_runner: RunnerClass,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut kind = None;
        let mut file = None;
        let mut repo = Some("local/repo".to_string());
        let mut commit = Some("local".to_string());
        let mut trust_tier = TrustTier::InternalBranch;
        let mut default_runner = RunnerClass::NativeRustClean;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--kind" => {
                    let value = value_after(args, i, "--kind")?;
                    kind = Some(parse_kind(value)?);
                    i += 2;
                }
                "--file" => {
                    file = Some(value_after(args, i, "--file")?.to_string());
                    i += 2;
                }
                "--repo" => {
                    repo = Some(value_after(args, i, "--repo")?.to_string());
                    i += 2;
                }
                "--commit" => {
                    commit = Some(value_after(args, i, "--commit")?.to_string());
                    i += 2;
                }
                "--trust-tier" => {
                    trust_tier = TrustTier::from_str(value_after(args, i, "--trust-tier")?)
                        .map_err(|err| err.to_string())?;
                    i += 2;
                }
                "--runner" => {
                    default_runner = RunnerClass::from_str(value_after(args, i, "--runner")?)
                        .map_err(|err| err.to_string())?;
                    i += 2;
                }
                other => return Err(format!("unknown option: {other}")),
            }
        }
        Ok(Self {
            kind: kind.ok_or_else(|| "--kind is required".to_string())?,
            file: file.ok_or_else(|| "--file is required".to_string())?,
            repo: repo.unwrap_or_else(|| "local/repo".to_string()),
            commit: commit.unwrap_or_else(|| "local".to_string()),
            trust_tier,
            default_runner,
        })
    }
}

fn value_after<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_kind(value: &str) -> Result<CiKind, String> {
    match value {
        "github" | "github-actions" | "actions" => Ok(CiKind::GitHubActions),
        "native" | "jit" | "toml" => Ok(CiKind::NativeToml),
        other => Err(format!("unknown CI kind: {other}")),
    }
}

fn explain(pipeline: &jeryu_ci_ir::Pipeline) -> Result<String, String> {
    let schedule = deterministic_schedule(pipeline).map_err(|err| err.to_string())?;
    let artifacts = plan_artifacts(pipeline).map_err(|err| err.to_string())?;
    let mut out = String::new();
    out.push_str(&format!("pipeline={}\n", pipeline.id));
    out.push_str(&format!("ir_hash={}\n", pipeline.ir_hash()));
    out.push_str(&format!("schedule_hash={}\n", schedule.schedule_hash));
    out.push_str("jobs:\n");
    for job in &pipeline.jobs {
        let cache = plan_cache_for_job(job, &pipeline.trust_tier);
        out.push_str(&format!(
            "- {} runner={} steps={} cache_fp={}\n",
            job.id,
            job.runner_class.as_str(),
            job.steps.len(),
            cache.fingerprint
        ));
    }
    out.push_str("rounds:\n");
    for round in &schedule.rounds {
        out.push_str(&format!("- {}: {}\n", round.index, round.jobs.join(",")));
    }
    out.push_str("artifacts:\n");
    for artifact in artifacts {
        out.push_str(&format!(
            "- {} job={} paths={}\n",
            artifact.name,
            artifact.job_id,
            artifact.paths.join(",")
        ));
    }
    Ok(out)
}

fn help() -> String {
    "jeryu-ci-bin <compile|schedule|explain> --kind <github|native> --file <path> [--repo owner/repo] [--commit sha] [--trust-tier internal] [--runner native-rust-clean]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kind_aliases() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(parse_kind("github")?, CiKind::GitHubActions);
        assert_eq!(parse_kind("native")?, CiKind::NativeToml);
        Ok(())
    }
}
