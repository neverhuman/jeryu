use super::*;
use crate::llm::{
    SecretResolver, SecretSource, provider_chains::load_providers_config, resolve_secret,
};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChainStatus {
    Ready,
    Partial,
    Missing,
}

impl ChainStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Partial => "partial",
            Self::Missing => "missing",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Ready => Color::Green,
            Self::Partial => Color::Yellow,
            Self::Missing => Color::Red,
        }
    }
}

#[derive(Debug, Clone)]
struct LlmPolicyRow {
    role: String,
    provider: String,
    model_id: String,
    key_source: String,
    chain_status: ChainStatus,
}

fn secret_source_label(source: SecretSource) -> &'static str {
    match source {
        SecretSource::Cli => "cli",
        SecretSource::Env => "env",
        SecretSource::UserDefault => "user-default",
        SecretSource::RepoLocal => "repo-local",
        SecretSource::NotFound => "missing",
    }
}

fn preferred_role_order() -> &'static [&'static str] {
    &[
        "security",
        "test_integrity",
        "runtime",
        "lockfile",
        "nightwatch",
    ]
}

fn collect_llm_rows(
    cfg: &crate::llm::provider_chains::ProvidersConfig,
    resolver: &SecretResolver,
) -> Vec<LlmPolicyRow> {
    let mut rows = Vec::new();
    let mut seen = HashSet::new();

    for &role in preferred_role_order() {
        if let Some(entries) = cfg.chains.get(role) {
            seen.insert(role.to_string());
            rows.extend(rows_for_role(role, entries, resolver));
        }
    }

    let mut extra_roles: Vec<&str> = cfg
        .chains
        .keys()
        .map(String::as_str)
        .filter(|role| !seen.contains(*role))
        .collect();
    extra_roles.sort_unstable();
    for role in extra_roles {
        if let Some(entries) = cfg.chains.get(role) {
            rows.extend(rows_for_role(role, entries, resolver));
        }
    }

    rows
}

fn rows_for_role(
    role: &str,
    entries: &[crate::llm::provider_chains::ProviderEntry],
    resolver: &SecretResolver,
) -> Vec<LlmPolicyRow> {
    let resolved = entries
        .iter()
        .filter(|entry| resolve_secret(&entry.api_key_secret, resolver).is_some())
        .count();
    let chain_status = if entries.is_empty() || resolved == 0 {
        ChainStatus::Missing
    } else if resolved == entries.len() {
        ChainStatus::Ready
    } else {
        ChainStatus::Partial
    };

    entries
        .iter()
        .map(|entry| {
            let key_source = resolve_secret(&entry.api_key_secret, resolver)
                .map(|resolved| secret_source_label(resolved.source))
                .unwrap_or("missing");
            LlmPolicyRow {
                role: role.to_string(),
                provider: entry.provider.clone(),
                model_id: entry.model_id.clone(),
                key_source: key_source.to_string(),
                chain_status,
            }
        })
        .collect()
}

pub(crate) fn draw_llms_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);

    focus::register_pane(app, PaneId::LLMsPolicyMatrix, cols[0]);
    focus::register_pane(app, PaneId::LLMsPolicySplit, cols[1]);

    let policy_path = app.autonomy_dir.join("providers").join("llm.yml");
    let resolver = app
        .llm_secret_resolver
        .clone()
        .unwrap_or_else(SecretResolver::from_env);
    let config_result = load_providers_config(&app.autonomy_dir);

    let (rows, summary_lines, _block_color, header_title) = match config_result {
        Ok(cfg) => {
            let rows = collect_llm_rows(&cfg, &resolver);
            let row_count = rows.len();
            let total_entries: usize = cfg.chains.values().map(Vec::len).sum();
            let resolved_entries = rows
                .iter()
                .filter(|row| row.key_source != "missing")
                .count();
            let default_role_chain = if cfg.default_role_chain.is_empty() {
                "none".to_string()
            } else {
                cfg.default_role_chain.join(" -> ")
            };
            let summary_lines = vec![
                Line::from(vec![
                    Span::styled("Policy:   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        short_text(&policy_path.display().to_string(), 44),
                        Style::default().fg(Color::White),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Keys:     ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "~/.jeryu/secrets/llm.env",
                        Style::default().fg(Color::Green),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Route:    ", Style::default().fg(Color::DarkGray)),
                    Span::styled(default_role_chain, Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled("Rows:     ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{} entries / {} resolved", total_entries, resolved_entries),
                        Style::default().fg(Color::White),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Split:    ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "policy in llm.yml, keys in ~/.jeryu/secrets",
                        Style::default().fg(Color::Magenta),
                    ),
                ]),
            ];
            (
                rows,
                summary_lines,
                Color::Cyan,
                format!(" [ LLM Policy Matrix ({row_count}) ] "),
            )
        }
        Err(err) => {
            let summary_lines = vec![
                Line::from(vec![
                    Span::styled("Policy:   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        policy_path.display().to_string(),
                        Style::default().fg(Color::Red),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Status:   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        short_text(&format!("{err:#}"), 48),
                        Style::default().fg(Color::Red),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Keys:     ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "~/.jeryu/secrets/llm.env",
                        Style::default().fg(Color::Green),
                    ),
                ]),
            ];
            (
                Vec::new(),
                summary_lines,
                Color::Red,
                " [ LLM Policy Matrix (error) ] ".to_string(),
            )
        }
    };

    let list_block = Block::default()
        .title(header_title)
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, PaneId::LLMsPolicyMatrix));

    let list_inner = list_block.inner(cols[0]);
    f.render_widget(list_block, cols[0]);

    let table_lines: Vec<ListItem> = if rows.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "  No LLM policy entries found.",
            Style::default().fg(Color::DarkGray),
        )]))]
    } else {
        let header = Line::from(vec![
            Span::styled(
                format!("{:<8} ", "role"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10} ", "provider"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<38} ", "model"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<12} ", "key source"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "chain",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let mut items = vec![ListItem::new(header)];
        items.extend(rows.into_iter().map(|row| {
            let status_color = row.chain_status.color();
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<8} ", short_text(&row.role, 8)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<10} ", short_text(&row.provider, 10)),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<38} ", short_text(&row.model_id, 38)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<12} ", short_text(&row.key_source, 12)),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    row.chain_status.label(),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        }));
        items
    };

    f.render_widget(
        List::new(table_lines).block(
            Block::default()
                .title(" [ LLM Role Wiring ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::LLMsPolicyMatrix)),
        ),
        list_inner,
    );

    let detail_block = Block::default()
        .title(" [ Model Policy Split ] ")
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, PaneId::LLMsPolicySplit));
    let detail_inner = detail_block.inner(cols[1]);
    f.render_widget(detail_block, cols[1]);

    let detail_lines = if summary_lines.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No LLM policy file found.",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        let mut lines = summary_lines;
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Resolved keys never render raw values.",
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            "  Source labels: cli, env, user-default, repo-local, missing.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  Free models only: OpenRouter routes the chain.",
            Style::default().fg(Color::Cyan),
        )));
        lines
    };

    f.render_widget(
        Paragraph::new(detail_lines)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        detail_inner,
    );
}
