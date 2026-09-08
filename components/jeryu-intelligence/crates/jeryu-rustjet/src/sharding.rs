use crate::error::{RustJetError, RustJetResult};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shard {
    pub index: usize,
    pub items: Vec<String>,
    pub estimated_cost: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardPlan {
    shards: Vec<Shard>,
}

impl ShardPlan {
    pub fn balanced(
        items: impl IntoIterator<Item = impl Into<String>>,
        shard_count: usize,
    ) -> RustJetResult<Self> {
        if shard_count == 0 {
            return Err(RustJetError::InvalidShardCount);
        }
        let mut shards: Vec<_> = (0..shard_count)
            .map(|index| Shard {
                index,
                items: Vec::new(),
                estimated_cost: 0,
            })
            .collect();
        let mut costs: BTreeMap<String, u64> = BTreeMap::new();
        for item in items {
            let item = item.into();
            costs.insert(item.clone(), deterministic_cost(&item));
        }
        let mut sorted: Vec<_> = costs.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        for (item, cost) in sorted {
            let target = shards
                .iter()
                .enumerate()
                .min_by_key(|(_, shard)| (shard.estimated_cost, shard.index))
                .map(|(index, _)| index)
                .unwrap_or(0);
            shards[target].estimated_cost += cost;
            shards[target].items.push(item);
        }
        for shard in &mut shards {
            shard.items.sort();
        }
        Ok(Self { shards })
    }

    #[must_use]
    pub fn shards(&self) -> &[Shard] {
        &self.shards
    }

    #[must_use]
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    #[must_use]
    pub fn max_cost_skew(&self) -> u64 {
        let Some(min) = self.shards.iter().map(|s| s.estimated_cost).min() else {
            return 0;
        };
        let Some(max) = self.shards.iter().map(|s| s.estimated_cost).max() else {
            return 0;
        };
        max - min
    }
}

fn deterministic_cost(item: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in item.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    1 + (hash % 100)
}
