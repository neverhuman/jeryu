//! Negative admission cases use scheduler time, never a worker's completion time.

use super::tests::{lease_book, result_for};
use super::{JobLeaseState, LeaseError};
use jeryu_runner_protocol::JobOutcome;

#[test]
fn idempotent_acquire_requires_the_same_runner_epoch() {
    let mut leases = lease_book(3);
    let active = leases
        .acquire_with_epoch("test", "worker-a", 7, 100, 30)
        .unwrap();
    for epoch in [6, 8] {
        assert!(matches!(
            leases.acquire_with_epoch("test", "worker-a", epoch, 110, 30),
            Err(LeaseError::FencedOut { .. })
        ));
        assert_eq!(
            leases.state("test"),
            Some(&JobLeaseState::Leased(active.clone()))
        );
    }
    assert_eq!(
        leases
            .acquire_with_epoch("test", "worker-a", 7, 110, 30)
            .unwrap(),
        active
    );
}

#[test]
fn expiry_cannot_bypass_the_one_attempt_default() {
    let mut leases = lease_book(1);
    leases.acquire("test", "worker-a", 100, 30).unwrap();
    assert!(matches!(
        leases.acquire("test", "worker-b", 130, 30),
        Err(LeaseError::PermanentlyFailed(_))
    ));
    assert!(matches!(
        leases.state("test"),
        Some(JobLeaseState::Failed { attempts: 1, .. })
    ));
}

#[test]
fn every_expired_lease_consumes_one_attempt() {
    let mut leases = lease_book(3);
    for attempt in 1..=3 {
        let lease = leases
            .acquire("test", "worker-a", 100 + 30 * (attempt - 1), 30)
            .unwrap();
        assert_eq!(lease.attempt, attempt as u32);
    }
    assert!(leases.acquire("test", "worker-a", 190, 30).is_err());
    assert!(matches!(
        leases.state("test"),
        Some(JobLeaseState::Failed { attempts: 3, .. })
    ));
}

#[test]
fn invalid_lifetimes_do_not_reserve_a_job() {
    for (at, ttl) in [(100, 0), (u64::MAX, 1), (u64::MAX - 1, 2)] {
        let mut leases = lease_book(1);
        assert!(leases.acquire("test", "worker-a", at, ttl).is_err());
        assert_eq!(leases.state("test"), Some(&JobLeaseState::Pending));
        assert_eq!(
            leases.acquire("test", "worker-a", 100, 30).unwrap().attempt,
            1
        );
    }
}

#[test]
fn results_require_current_server_time_within_the_lease() {
    for outcome in [
        JobOutcome::Success,
        JobOutcome::Failed,
        JobOutcome::Cancelled,
        JobOutcome::TimedOut,
        JobOutcome::InfrastructureFailure,
    ] {
        for at in [99, 130, 131] {
            let mut leases = lease_book(3);
            let lease = leases.acquire("test", "worker-a", 100, 30).unwrap();
            let result = result_for(&lease, outcome.clone());
            assert!(
                leases.apply_result(&result, at).is_err(),
                "accepted {outcome:?} at {at}"
            );
            assert_eq!(leases.state("test"), Some(&JobLeaseState::Leased(lease)));
        }
    }
}

#[test]
fn cancellation_is_terminal_even_with_unused_retry_budget() {
    let mut leases = lease_book(3);
    let lease = leases.acquire("test", "worker-a", 100, 30).unwrap();
    let result = result_for(&lease, JobOutcome::Cancelled);
    let receipt = leases.apply_result(&result, 120).unwrap();
    assert_eq!(receipt.kind.as_str(), "cancelled");
    assert!(leases.acquire("test", "worker-a", 121, 30).is_err());
    assert!(leases.apply_result(&result, 121).is_err());
}

#[test]
fn previous_attempt_results_cannot_complete_a_reissued_lease() {
    let mut leases = lease_book(2);
    let first = leases.acquire("test", "worker-a", 100, 30).unwrap();
    let second = leases.acquire("test", "worker-a", 130, 30).unwrap();
    assert_eq!(second.attempt, 2);
    assert_ne!(first.id, second.id);
    assert!(
        leases
            .apply_result(&result_for(&first, JobOutcome::Success), 140)
            .is_err()
    );
    let result = result_for(&second, JobOutcome::Success);
    leases.apply_result(&result, 140).unwrap();
    assert!(leases.apply_result(&result, 141).is_err());
    assert_eq!(leases.state("test"), Some(&JobLeaseState::Succeeded));
}

#[test]
fn direct_transitions_reject_tampered_lease_fields() {
    for field in ["worker", "epoch", "attempt", "expiry"] {
        let mut leases = lease_book(3);
        let original = leases.acquire("test", "worker-a", 100, 30).unwrap();
        let mut forged = original.clone();
        match field {
            "worker" => forged.worker_id = "worker-b".into(),
            "epoch" => forged.node_epoch += 1,
            "attempt" => forged.attempt += 1,
            "expiry" => forged.expires_at_epoch += 1,
            _ => unreachable!(),
        }
        assert!(
            leases.complete(&forged, 120).is_err(),
            "completed forged {field}"
        );
        assert!(
            leases.fail(&forged, "failed", 120).is_err(),
            "failed forged {field}"
        );
        assert!(leases.cancel(&forged, "cancelled", 120).is_err());
        assert_eq!(leases.state("test"), Some(&JobLeaseState::Leased(original)));
    }
}

#[test]
fn direct_transitions_cannot_bypass_expiry_or_backdate_results() {
    for at in [99, 130, u64::MAX] {
        let mut leases = lease_book(3);
        let lease = leases.acquire("test", "worker-a", 100, 30).unwrap();
        assert!(leases.complete(&lease, at).is_err());
        assert!(leases.fail(&lease, "failed", at).is_err());
        assert!(leases.cancel(&lease, "cancelled", at).is_err());
        assert_eq!(leases.state("test"), Some(&JobLeaseState::Leased(lease)));
    }
}

#[test]
fn expiry_sweep_emits_one_receipt_per_attempt_and_never_reopens_terminal_jobs() {
    for budget in [1, 3] {
        let mut leases = lease_book(budget);
        for attempt in 1..=budget {
            let at = 100 + 30 * u64::from(attempt - 1);
            let lease = leases.acquire("test", "worker-a", at, 30).unwrap();
            assert_eq!(lease.attempt, attempt);
            assert!(leases.expire(at + 29).is_empty());
            let receipts = leases.expire(at + 30);
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].attempt, attempt);
            assert_eq!(receipts[0].lease_id, lease.id);
            assert_eq!(receipts[0].at_epoch, at + 30);
            assert_eq!(receipts[0].reason, "lease expired");
            assert_eq!(
                receipts[0].kind.as_str(),
                if attempt < budget {
                    "requeued"
                } else {
                    "failed"
                }
            );
            assert!(receipts[0].result_hash.is_none());
            assert!(leases.expire(at + 30).is_empty());
            assert!(
                leases
                    .apply_result(&result_for(&lease, JobOutcome::Success), at + 30)
                    .is_err()
            );
        }
        assert!(leases.acquire("test", "worker-b", 500, 30).is_err());
        assert!(leases.expire(500).is_empty());
    }
}

#[test]
fn expiry_sweep_leaves_success_and_cancellation_terminal() {
    for outcome in [JobOutcome::Success, JobOutcome::Cancelled] {
        let mut leases = lease_book(3);
        let lease = leases.acquire("test", "worker-a", 100, 30).unwrap();
        leases
            .apply_result(&result_for(&lease, outcome), 129)
            .unwrap();
        let terminal = leases.state("test").cloned();
        assert!(leases.expire(130).is_empty());
        assert!(leases.expire(u64::MAX).is_empty());
        assert!(leases.acquire("test", "worker-a", 140, 30).is_err());
        assert_eq!(leases.state("test"), terminal.as_ref());
    }
}
