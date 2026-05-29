use super::parse_capture_tab;
use crate::tui::app::ActiveTab;

#[test]
fn parse_capture_tab_accepts_primary_tabs_and_aliases() {
    let cases = [
        ("workflow", ActiveTab::Workflow),
        ("0", ActiveTab::Workflow),
        ("mission", ActiveTab::Mission),
        ("release", ActiveTab::Release),
        ("approvals", ActiveTab::Approvals),
        ("jobs", ActiveTab::Jobs),
        ("flow", ActiveTab::Jobs),
        ("agents", ActiveTab::Agents),
        ("tests", ActiveTab::Tests),
        ("vti", ActiveTab::Tests),
        ("pools", ActiveTab::Pools),
        ("cache", ActiveTab::Cache),
        ("evidence", ActiveTab::Evidence),
        ("audit", ActiveTab::Evidence),
        ("repos", ActiveTab::Repos),
        ("repositories", ActiveTab::Repos),
        ("families", ActiveTab::Repos),
        ("bugs", ActiveTab::Bugs),
        ("bug", ActiveTab::Bugs),
        ("llms", ActiveTab::LLMs),
        ("llm", ActiveTab::LLMs),
        ("secrets", ActiveTab::Secrets),
        ("git", ActiveTab::Git),
        ("jankurai", ActiveTab::Jankurai),
        ("jank", ActiveTab::Jankurai),
        ("quality", ActiveTab::Jankurai),
    ];

    for (input, expected) in cases {
        assert_eq!(parse_capture_tab(input).unwrap(), expected);
        assert_eq!(
            parse_capture_tab(&input.to_ascii_uppercase()).unwrap(),
            expected
        );
    }
}

#[test]
fn parse_capture_tab_rejects_unknown_tab() {
    let error = parse_capture_tab("unknown").unwrap_err().to_string();
    assert!(error.contains("unknown TUI tab"));
    assert!(error.contains("workflow"));
}
