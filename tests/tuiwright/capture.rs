use super::harness::{
    assert_main_layout_regions, assert_png_shape_and_ink, capture_tui, read_png, tuiwright_lock,
};

#[test]
fn capture_path_renders_all_primary_tabs_split() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    for tab in [
        "workflow",
        "mission",
        "release",
        "approvals",
        "jobs",
        "agents",
        "tests",
        "pools",
        "cache",
        "evidence",
        "repos",
        "bugs",
        "secrets",
        "llms",
        "git",
        "jankurai",
    ] {
        let path = capture_tui(tab)?;
        let image = read_png(&path)?;
        assert_png_shape_and_ink(&path, &image);
        assert_main_layout_regions(tab, &image);
    }
    Ok(())
}
