use crate::api::{
    entity::EntityRef,
    read_model::{QueueJobSummary, QueuePoolSnapshot, QueueSnapshot, TuiReadModel},
};

#[derive(Debug, Clone, Copy)]
pub struct QueueLensInput<'a> {
    pub model: &'a TuiReadModel,
}

pub fn select_queue_lens_input(model: &TuiReadModel) -> QueueLensInput<'_> {
    QueueLensInput { model }
}

impl<'a> QueueLensInput<'a> {
    pub fn queue(self) -> &'a QueueSnapshot {
        &self.model.queue
    }

    pub fn pools(self) -> &'a [QueuePoolSnapshot] {
        &self.model.queue.pools
    }

    pub fn waiting_jobs(self) -> Vec<&'a QueueJobSummary> {
        self.model
            .queue
            .waiting_jobs
            .iter()
            .filter(|job| job.is_waiting())
            .collect::<Vec<_>>()
    }

    pub fn stage_summaries(self) -> Vec<QueueStageSummary> {
        let mut stages = Vec::new();
        for job in &self.model.queue.waiting_jobs {
            let index = stages
                .iter()
                .position(|stage: &QueueStageSummary| stage.stage == job.stage);
            let summary = match index {
                Some(index) => &mut stages[index],
                None => {
                    stages.push(QueueStageSummary {
                        stage: job.stage.clone(),
                        queued: 0,
                        running: 0,
                        avg_queue_secs: 0,
                    });
                    stages.last_mut().expect("stage was just pushed")
                }
            };
            if job.is_waiting() {
                summary.queued += 1;
                summary.avg_queue_secs += job.queued_secs();
            }
        }
        for pool in &self.model.queue.pools {
            if pool.running_jobs == 0 {
                continue;
            }
            let stage = pool.name.clone();
            let summary = match stages
                .iter()
                .position(|summary: &QueueStageSummary| summary.stage == stage)
            {
                Some(index) => &mut stages[index],
                None => {
                    stages.push(QueueStageSummary {
                        stage,
                        queued: 0,
                        running: 0,
                        avg_queue_secs: 0,
                    });
                    stages.last_mut().expect("stage was just pushed")
                }
            };
            summary.running += pool.running_jobs;
        }
        for stage in &mut stages {
            if stage.queued > 0 {
                stage.avg_queue_secs /= u64::from(stage.queued);
            }
        }
        stages
    }

    pub fn first_pool_entity(self) -> Option<EntityRef> {
        self.model
            .queue
            .pools
            .first()
            .map(|pool| pool.entity.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueStageSummary {
    pub stage: String,
    pub queued: u32,
    pub running: u32,
    pub avg_queue_secs: u64,
}
