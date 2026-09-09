use crate::classifier::{AffectedPlan, CiCommand, ImpactReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainFormat {
    Json,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainOutput {
    pub body: String,
}

impl ExplainOutput {
    #[must_use]
    pub fn render(plan: &AffectedPlan, format: ExplainFormat) -> Self {
        let body = match format {
            ExplainFormat::Json => render_json(plan),
            ExplainFormat::Text => render_text(plan),
        };
        Self { body }
    }
}

fn render_text(plan: &AffectedPlan) -> String {
    let packages = plan
        .affected_package_names()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    let reasons = plan
        .reasons
        .iter()
        .map(ImpactReason::code)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "runner={}\nsccache={}\nfail_closed={}\nreasons={}\naffected={}\ncommands={}\n",
        plan.runner_class.as_str(),
        plan.sccache_mode,
        plan.fail_closed,
        reasons,
        packages,
        plan.commands
            .iter()
            .map(command_to_text)
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

fn render_json(plan: &AffectedPlan) -> String {
    let packages = plan
        .affected_packages
        .iter()
        .map(|package| {
            format!(
                "{{\"name\":\"{}\",\"reasons\":[{}]}}",
                escape(&package.name),
                package
                    .reasons
                    .iter()
                    .map(|reason| format!("\"{}\"", reason.code()))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let reasons = plan
        .reasons
        .iter()
        .map(|reason| format!("\"{}\"", reason.code()))
        .collect::<Vec<_>>()
        .join(",");
    let lanes = plan
        .proof_lanes
        .iter()
        .map(|lane| format!("\"{}\"", escape(lane)))
        .collect::<Vec<_>>()
        .join(",");
    let commands = plan
        .commands
        .iter()
        .map(command_to_json)
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "{{\"runner_class\":\"{}\",\"sccache_mode\":\"{}\",\"fail_closed\":{},\"reasons\":[{}],\"proof_lanes\":[{}],\"affected_packages\":[{}],\"commands\":[{}]}}",
        plan.runner_class.as_str(),
        escape(&plan.sccache_mode),
        plan.fail_closed,
        reasons,
        lanes,
        packages,
        commands
    )
}

fn command_to_text(command: &CiCommand) -> String {
    format!("{}:{}", command.lane, command.argv.join(" "))
}

fn command_to_json(command: &CiCommand) -> String {
    let argv = command
        .argv
        .iter()
        .map(|arg| format!("\"{}\"", escape(arg)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"lane\":\"{}\",\"argv\":[{}]}}",
        escape(&command.lane),
        argv
    )
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
