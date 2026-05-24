use super::*;

#[test]
fn project_ref_url_encodes_slug() {
    let repo = RepoRef::parse("root/veox").unwrap();
    assert_eq!(GitLabClient::project_ref(&repo), "root%2Fveox");
}

#[test]
fn diff_line_counts_ignore_file_headers() {
    let diff = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n+plus\n";
    assert_eq!(count_diff_lines(diff), (2, 1));
}

#[test]
fn canonical_status_name_is_gitlab_visible_gate() {
    assert_eq!(
        VIBEGATE_MERGE_PASSPORT_CHECK_NAME,
        "vibegate/merge-passport"
    );
}
