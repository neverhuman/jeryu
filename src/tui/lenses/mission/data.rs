use crate::api::{
    entity::{BlockerSummary, HealthLevel},
    freshness::SourceFreshness,
    inspection::InspectionEnvelope,
    read_model::{AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel},
};

#[derive(Debug, Clone, Copy)]
pub struct MissionLensInput<'a> {
    pub model: &'a TuiReadModel,
    pub sources: &'a [SourceFreshness],
}

pub fn select_mission_lens_input(
    envelope: &InspectionEnvelope<TuiReadModel>,
) -> MissionLensInput<'_> {
    MissionLensInput {
        model: &envelope.data,
        sources: &envelope.sources,
    }
}

impl<'a> MissionLensInput<'a> {
    pub fn mission(self) -> &'a MissionSnapshot {
        &self.model.mission
    }

    pub fn attention(self) -> &'a [AttentionItem] {
        &self.model.attention
    }

    pub fn next_action(self) -> Option<&'a NextActionRecommendation> {
        self.model.next_action.as_ref()
    }

    pub fn top_blocker(self) -> Option<&'a BlockerSummary> {
        self.model.mission.top_blocker.as_ref()
    }

    pub fn posture_status(self) -> &'static str {
        let mission = self.mission();
        if !mission.safe_to_code || mission.overall == HealthLevel::Critical {
            "blocked"
        } else if !mission.safe_to_merge || mission.top_blocker.is_some() {
            "waiting"
        } else if mission.safe_to_release {
            "success"
        } else {
            "running"
        }
    }

    pub fn posture_label(self) -> String {
        let mission = self.mission();
        if !mission.safe_to_code {
            "BLOCKED: code work paused".into()
        } else if !mission.safe_to_merge {
            "CAUTION: merge gate incomplete".into()
        } else if mission.safe_to_release {
            "READY: release gate open".into()
        } else {
            "READY: delivery work can continue".into()
        }
    }

    pub fn primary_freshness(self) -> Option<&'a SourceFreshness> {
        self.sources
            .iter()
            .find(|source| source.state.blocks_risky_action())
            .or_else(|| self.sources.first())
    }

    pub fn proof_links(self) -> Vec<&'a str> {
        let mut links = Vec::new();
        for proof in self
            .model
            .attention
            .iter()
            .flat_map(|item| item.evidence.iter().map(String::as_str))
        {
            if !links.contains(&proof) {
                links.push(proof);
            }
        }
        links
    }
}
