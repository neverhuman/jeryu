use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::runtime::Builder;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;

use super::{RuntimeVariant, build_result};
use crate::model::BenchVariantResult;

pub(crate) fn baseline_runtime(
    ops: usize,
    workers: usize,
    key_space: u64,
) -> Result<BenchVariantResult> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let shared = Arc::new(Mutex::new(HashMap::<u64, u64>::new()));
    let ops_per_worker = ops / workers.max(1);
    let start = Instant::now();
    let mut handles = Vec::new();
    for worker in 0..workers {
        let shared = Arc::clone(&shared);
        handles.push(thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_worker);
            for step in 0..ops_per_worker {
                let key = ((worker * 37 + step) as u64) % key_space;
                let op_start = Instant::now();
                {
                    let mut map = shared.lock().expect("lock");
                    let entry = map.entry(key).or_insert(0);
                    *entry += 1;
                }
                latencies.push(op_start.elapsed().as_secs_f64() * 1000.0);
            }
            latencies
        }));
    }
    let mut latencies = Vec::new();
    for handle in handles {
        latencies.extend(handle.join().expect("worker panic"));
    }
    let wall = start.elapsed();
    Ok(build_result(
        RuntimeVariant::Baseline,
        wall,
        workers as u64,
        ops as f64 / wall.as_secs_f64(),
        latencies,
        vec!["Shared-state baseline with lock contention under concurrent mutation.".to_string()],
    ))
}

pub(crate) fn actor_runtime(
    ops: usize,
    workers: usize,
    key_space: u64,
) -> Result<BenchVariantResult> {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build current-thread runtime")?;
    let local = LocalSet::new();
    let result = local.block_on(&runtime, async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<(u64, oneshot::Sender<u64>)>();
        let actor = tokio::task::spawn_local(async move {
            let mut map = HashMap::<u64, u64>::new();
            while let Some((key, reply)) = rx.recv().await {
                let entry = map.entry(key).or_insert(0);
                *entry += 1;
                let _ = reply.send(*entry);
            }
        });
        let ops_per_worker = ops / workers.max(1);
        let start = Instant::now();
        let mut tasks = Vec::new();
        for worker in 0..workers {
            let tx = tx.clone();
            tasks.push(tokio::task::spawn_local(async move {
                let mut latencies = Vec::with_capacity(ops_per_worker);
                for step in 0..ops_per_worker {
                    let key = ((worker * 37 + step) as u64) % key_space;
                    let op_start = Instant::now();
                    let (reply_tx, reply_rx) = oneshot::channel();
                    tx.send((key, reply_tx)).expect("actor channel open");
                    let _ = reply_rx.await.expect("actor reply");
                    latencies.push(op_start.elapsed().as_secs_f64() * 1000.0);
                }
                latencies
            }));
        }
        drop(tx);
        let mut latencies = Vec::new();
        for task in tasks {
            latencies.extend(task.await.expect("task panic"));
        }
        actor.await.expect("actor panic");
        let wall = start.elapsed();
        build_result(
            RuntimeVariant::ActorAsync,
            wall,
            1,
            ops as f64 / wall.as_secs_f64(),
            latencies,
            vec![
                "Single-thread actor runtime with message-passing instead of shared locks."
                    .to_string(),
            ],
        )
    });
    Ok(result)
}
