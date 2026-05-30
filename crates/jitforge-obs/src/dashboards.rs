//! Dashboard rendering.

use crate::slo::phase10_slos;

/// One dashboard panel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DashboardPanel {
    pub title: String,
    pub query: String,
}

/// Generate a Grafana-compatible JSON dashboard for Phase 10 SLOs.
pub fn phase10_grafana_dashboard() -> String {
    let panels: Vec<DashboardPanel> = phase10_slos()
        .iter()
        .map(|slo| DashboardPanel {
            title: format!("{} ({})", slo.name, slo.window),
            query: slo.query.to_owned(),
        })
        .collect();

    let mut json = String::from(
        "{\n  \"title\": \"JitForge Nitro Phase 10\",\n  \"schemaVersion\": 39,\n  \"panels\": [\n",
    );
    for (index, panel) in panels.iter().enumerate() {
        let comma = if index + 1 == panels.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{ \"id\": {}, \"title\": \"{}\", \"type\": \"timeseries\", \"targets\": [{{ \"expr\": \"{}\" }}] }}{}\n",
            index + 1,
            escape(&panel.title),
            escape(&panel.query),
            comma
        ));
    }
    json.push_str("  ]\n}\n");
    json
}

fn escape(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}
