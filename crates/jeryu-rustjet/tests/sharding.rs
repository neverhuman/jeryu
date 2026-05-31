use jeryu_rustjet::{FeatureSelection, NextestPlanner, ShardPlan};

#[test]
fn sharding_is_deterministic() {
    let items = [
        "a::test_one",
        "b::test_two",
        "c::test_three",
        "d::test_four",
        "e::test_five",
    ];
    let first = ShardPlan::balanced(items, 3).expect("first shard plan");
    let second = ShardPlan::balanced(items, 3).expect("second shard plan");
    assert_eq!(first, second);
    assert_eq!(first.shard_count(), 3);
}

#[test]
fn nextest_command_includes_partition() {
    let shard_plan = ShardPlan::balanced(["a", "b", "c"], 2).expect("shards");
    let planner = NextestPlanner::new(FeatureSelection::explicit(["fast"]));
    let cmd = planner.command_for_shard(&["core".to_string()], &shard_plan, 0);
    let text = cmd.display();
    assert!(text.contains("cargo nextest run"));
    assert!(text.contains("--package core"));
    assert!(text.contains("--partition count:1/2"));
    assert!(text.contains("--features fast"));
}
