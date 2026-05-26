use super::*;

fn req(
    id: &str,
    project_id: i64,
    priority: SchedulerPriority,
    reason: SchedulerReason,
    submitted_at: u64,
) -> SchedulerRequest {
    SchedulerRequest {
        id: id.to_string(),
        project_id,
        priority,
        reason,
        submitted_at,
    }
}

fn ids(items: &[ScheduledRequest]) -> Vec<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

#[test]
fn round_robins_equal_priority_across_projects() {
    let requests = vec![
        req(
            "a1",
            1,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            1,
        ),
        req(
            "a2",
            1,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            2,
        ),
        req(
            "a3",
            1,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            3,
        ),
        req(
            "b1",
            2,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            4,
        ),
        req(
            "c1",
            3,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            5,
        ),
    ];

    let scheduled = schedule_requests(&requests, 5, 10, SchedulerPolicy::default());

    assert_eq!(ids(&scheduled), vec!["a1", "b1", "c1", "a2", "a3"]);
}

#[test]
fn override_priority_preempts_older_normal_work() {
    let requests = vec![
        req(
            "normal-old",
            1,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            1,
        ),
        req(
            "high",
            2,
            SchedulerPriority::High,
            SchedulerReason::TestFix,
            2,
        ),
        req(
            "override",
            3,
            SchedulerPriority::Override,
            SchedulerReason::CherryPick,
            3,
        ),
    ];

    let scheduled = schedule_requests(&requests, 3, 10, SchedulerPolicy::default());

    assert_eq!(ids(&scheduled), vec!["override", "high", "normal-old"]);
}

#[test]
fn urgent_reasons_win_within_the_same_project_and_priority() {
    let requests = vec![
        req(
            "general",
            1,
            SchedulerPriority::High,
            SchedulerReason::General,
            1,
        ),
        req(
            "cherry",
            1,
            SchedulerPriority::High,
            SchedulerReason::CherryPick,
            2,
        ),
        req(
            "test-fix",
            1,
            SchedulerPriority::High,
            SchedulerReason::TestFix,
            3,
        ),
        req(
            "release",
            1,
            SchedulerPriority::High,
            SchedulerReason::ReleaseFix,
            4,
        ),
    ];

    let scheduled = schedule_requests(&requests, 4, 10, SchedulerPolicy::default());

    assert_eq!(
        ids(&scheduled),
        vec!["release", "test-fix", "cherry", "general"]
    );
}

#[test]
fn old_normal_work_ages_into_high_priority() {
    let requests = vec![
        req(
            "fresh-high",
            1,
            SchedulerPriority::High,
            SchedulerReason::General,
            90,
        ),
        req(
            "old-normal",
            2,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            1,
        ),
        req(
            "fresh-normal",
            3,
            SchedulerPriority::Normal,
            SchedulerReason::General,
            95,
        ),
    ];
    let policy = SchedulerPolicy {
        age_boost_after: Some(50),
    };

    let scheduled = schedule_requests(&requests, 3, 100, policy);

    assert_eq!(
        ids(&scheduled),
        vec!["old-normal", "fresh-high", "fresh-normal"]
    );
    assert_eq!(scheduled[0].priority, SchedulerPriority::High);
}
