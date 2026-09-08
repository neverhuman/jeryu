//! Focused raw-byte ring, cursor, and broadcast regression tests.

use base64::Engine;

use super::{AgentRunEventInput, AgentRunStore, test_raw_tty_event};

fn decode(event: &super::AgentTtyEvent) -> Vec<u8> {
    let encoded = event.bytes_b64.as_deref().expect("raw bytes payload");
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("base64 decode")
}

#[test]
fn tail_returns_events_past_cursor_with_raw_bytes_intact() {
    let store = AgentRunStore::new();
    store.seed_test_run("ar-tail", 16);
    // A deliberately non-UTF8 byte sequence to prove byte-for-byte fidelity.
    let raw_two = [0xff_u8, 0x00, 0xfe, b'h', b'i', 0x80];
    store.push_test_tty("ar-tail", test_raw_tty_event("ar-tail", 1, b"one"));
    store.push_test_tty("ar-tail", test_raw_tty_event("ar-tail", 2, &raw_two));
    store.push_test_tty("ar-tail", test_raw_tty_event("ar-tail", 3, b"three"));

    let tail = store.tail_tty("ar-tail", 1, None).expect("tail result");
    assert!(!tail.lagged);
    assert_eq!(tail.after_seq, 1);
    assert_eq!(tail.next_after_seq, 3);
    let seqs: Vec<u64> = tail.events.iter().map(|event| event.seq).collect();
    assert_eq!(seqs, vec![2, 3], "only events strictly after the cursor");
    assert_eq!(
        decode(&tail.events[0]),
        raw_two,
        "non-UTF8 bytes round-trip byte-identical through bytes_b64"
    );
}

#[test]
fn tail_from_zero_advances_then_returns_only_new_events() {
    let store = AgentRunStore::new();
    store.seed_test_run("ar-cursor", 16);
    store.push_test_tty("ar-cursor", test_raw_tty_event("ar-cursor", 1, b"a"));
    store.push_test_tty("ar-cursor", test_raw_tty_event("ar-cursor", 2, b"b"));

    let first = store.tail_tty("ar-cursor", 0, None).expect("first tail");
    assert!(!first.lagged);
    assert_eq!(
        first.events.len(),
        2,
        "after_seq=0 starts from the beginning"
    );
    assert_eq!(first.next_after_seq, 2);

    let empty = store
        .tail_tty("ar-cursor", first.next_after_seq, None)
        .expect("empty tail");
    assert!(empty.events.is_empty(), "no events newer than the cursor");
    assert_eq!(
        empty.next_after_seq, 2,
        "next cursor holds when nothing is newer"
    );

    store.push_test_tty("ar-cursor", test_raw_tty_event("ar-cursor", 3, b"c"));
    let resumed = store
        .tail_tty("ar-cursor", empty.next_after_seq, None)
        .expect("resumed tail");
    let seqs: Vec<u64> = resumed.events.iter().map(|event| event.seq).collect();
    assert_eq!(
        seqs,
        vec![3],
        "second tail returns only the freshly pushed event"
    );
}

#[test]
fn ring_eviction_marks_lagged_from_evicted_cursor_only() {
    let store = AgentRunStore::new();
    store.seed_test_run("ar-ring", 4);
    // Push six events into a four-slot ring: seq 1 and 2 roll off the front.
    for seq in 1..=6 {
        store.push_test_tty(
            "ar-ring",
            test_raw_tty_event("ar-ring", seq, format!("chunk-{seq}").as_bytes()),
        );
    }

    let evicted = store.tail_tty("ar-ring", 1, None).expect("evicted tail");
    assert!(evicted.lagged, "a cursor behind the ring is flagged lagged");
    assert_eq!(
        evicted.oldest_retained_seq, 3,
        "oldest retained seq after eviction"
    );
    let seqs: Vec<u64> = evicted.events.iter().map(|event| event.seq).collect();
    assert_eq!(
        seqs,
        vec![3, 4, 5, 6],
        "a lagged tail resyncs from the oldest retained event"
    );

    let live = store.tail_tty("ar-ring", 4, None).expect("live tail");
    assert!(!live.lagged, "a cursor inside the ring is not lagged");
    let live_seqs: Vec<u64> = live.events.iter().map(|event| event.seq).collect();
    assert_eq!(live_seqs, vec![5, 6]);

    // A cursor exactly on the last evicted seq is contiguous, not lagged.
    let boundary = store.tail_tty("ar-ring", 2, None).expect("boundary tail");
    assert!(!boundary.lagged);
}

#[test]
fn append_event_still_feeds_the_ws_snapshot_feed() {
    let store = AgentRunStore::new();
    store.seed_test_run("ar-ws", 16);
    store.append_event(
        "ar-ws",
        AgentRunEventInput {
            kind: "tty",
            stream: Some("stdout"),
            text: Some("ws-visible-line".to_string()),
            pid: None,
            used: None,
            limit: None,
            exit_code: None,
            timed_out: false,
            budget_exceeded: false,
        },
    );

    // The WS tty stream renders from status(); the appended event must show up.
    let status = store.status("ar-ws").expect("status snapshot");
    assert!(
        status
            .tty_events
            .iter()
            .any(|event| event.text.as_deref() == Some("ws-visible-line")),
        "append_event remains the publish point feeding WS subscribers"
    );
    // The same event is tailable as a raw cursor-pull payload.
    let tail = store.tail_tty("ar-ws", 0, None).expect("tail snapshot");
    assert_eq!(tail.events.len(), 1);
    assert!(!tail.lagged);
}

#[test]
fn append_event_fans_out_to_a_live_broadcast_subscriber() {
    let store = AgentRunStore::new();
    store.seed_test_run("ar-fanout", 16);
    // Open a live subscription first, then publish through the one publish point.
    let (mut receiver, replay) = store.tty_stream_start("ar-fanout", 0).expect("subscribe");
    assert!(
        replay.events.is_empty(),
        "no buffered events before publish"
    );

    store.append_event(
        "ar-fanout",
        AgentRunEventInput {
            kind: "tty",
            stream: Some("stdout"),
            text: Some("fanout-line".to_string()),
            pid: None,
            used: None,
            limit: None,
            exit_code: None,
            timed_out: false,
            budget_exceeded: false,
        },
    );

    let live = receiver
        .try_recv()
        .expect("append_event fans the event out to the live broadcast");
    assert_eq!(live.text.as_deref(), Some("fanout-line"));
    // The same event still sits in the ring for a resyncing cursor-pull tailer.
    let tail = store.tail_tty("ar-fanout", 0, None).expect("tail snapshot");
    assert_eq!(tail.events.len(), 1);
    assert_eq!(tail.events[0].text.as_deref(), Some("fanout-line"));
}
