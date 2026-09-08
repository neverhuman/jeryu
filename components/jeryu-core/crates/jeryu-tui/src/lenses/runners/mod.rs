//! Runners lens — operator Pools/Health for the runner-pool fleet.
//!
//! Two pure projections of the read model: the per-pool [`PoolHealthInput`]
//! (fleet totals, per-pool utilization/slots/jobs, ranked bottleneck + health
//! banner) from [`PoolActivity`](jeryu_readmodel::PoolActivity), plus the
//! legacy per-node [`RunnersLensInput`] grid from the runners dashboard.

pub mod data;
pub mod view;

pub use data::{PoolHealthInput, PoolHealthRow, RunnerNodeRow, RunnersLensInput};
pub use view::draw;
