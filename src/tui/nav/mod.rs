//! Owner: Interactive TUI subsystem - Flight Deck navigation
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: Navigation returns route intents; it does not mutate app state.

use crate::api::entity::EntityRef;

mod direction;

pub use direction::NavDirection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteIntent {
    Push(Route),
    Pop,
    Focus(EntityRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub lens: &'static str,
    pub entity: Option<EntityRef>,
}

impl Route {
    pub fn lens(lens: &'static str) -> Self {
        Self { lens, entity: None }
    }

    pub fn entity(lens: &'static str, entity: EntityRef) -> Self {
        Self {
            lens,
            entity: Some(entity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef};

    #[test]
    fn route_can_target_entity() {
        let entity = EntityRef::new(EntityKind::Job, "42");
        let route = Route::entity("workflow", entity.clone());
        assert_eq!(route.lens, "workflow");
        assert_eq!(route.entity, Some(entity));
    }
}
