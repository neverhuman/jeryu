//! Users, organizations, and teams.

use chrono::Utc;
use uuid::Uuid;

use super::{ForgeCore, require_name, slugify};
use crate::errors::{ForgeError, Result};
use crate::model::*;

impl ForgeCore {
    pub fn create_user(&self, request: CreateUserRequest) -> Result<User> {
        self.with_global_mutation(|| self.create_user_admitted(request))
    }

    pub(super) fn create_user_admitted(&self, request: CreateUserRequest) -> Result<User> {
        require_name("login", &request.login)?;
        let mut state = self.runtime.state.write();
        if state.users.contains_key(&request.login) {
            return Err(ForgeError::Conflict(format!("user {}", request.login)));
        }
        let previous = state.clone();
        let user = User {
            id: Uuid::new_v4(),
            login: request.login.clone(),
            name: request.name,
            email: request.email,
            created_at: Utc::now(),
        };
        state.users.insert(request.login, user.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(user)
    }

    pub fn get_user(&self, login: &str) -> Result<User> {
        self.runtime
            .state
            .read()
            .users
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("user {login}")))
    }

    /// Return or create a display profile, propagating writer admission errors.
    /// A profile is not an authenticated account or an authorization grant.
    pub fn ensure_user(&self, login: &str) -> Result<User> {
        self.with_global_mutation(|| self.ensure_user_admitted(login))
    }

    pub(super) fn ensure_user_admitted(&self, login: &str) -> Result<User> {
        if let Ok(user) = self.get_user(login) {
            return Ok(user);
        }
        self.create_user_admitted(CreateUserRequest {
            login: login.to_string(),
            name: None,
            email: None,
        })
    }

    pub fn create_organization(&self, request: CreateOrganizationRequest) -> Result<Organization> {
        self.with_global_mutation(|| {
            require_name("login", &request.login)?;
            let mut state = self.runtime.state.write();
            if state.organizations.contains_key(&request.login) {
                return Err(ForgeError::Conflict(format!(
                    "organization {}",
                    request.login
                )));
            }
            let previous = state.clone();
            let organization = Organization {
                id: Uuid::new_v4(),
                login: request.login.clone(),
                display_name: request.display_name,
                created_at: Utc::now(),
            };
            state
                .organizations
                .insert(request.login, organization.clone());
            self.persist_after_mutation(&mut state, previous)?;
            Ok(organization)
        })
    }

    pub fn get_organization(&self, login: &str) -> Result<Organization> {
        self.runtime
            .state
            .read()
            .organizations
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("organization {login}")))
    }

    pub fn create_team(&self, org: &str, request: CreateTeamRequest) -> Result<Team> {
        self.with_global_mutation(|| {
            require_name("team name", &request.name)?;
            let slug = match request.slug {
                Some(slug) => slug,
                None => slugify(&request.name),
            };
            require_name("team slug", &slug)?;
            let mut state = self.runtime.state.write();
            if !state.organizations.contains_key(org) {
                return Err(ForgeError::NotFound(format!("organization {org}")));
            }
            let key = (org.to_string(), slug.clone());
            if state.teams.contains_key(&key) {
                return Err(ForgeError::Conflict(format!("team {org}/{slug}")));
            }
            let previous = state.clone();
            let team = Team {
                id: Uuid::new_v4(),
                organization: org.to_string(),
                slug: slug.clone(),
                name: request.name,
                members: request.members,
                created_at: Utc::now(),
            };
            state.teams.insert(key, team.clone());
            self.persist_after_mutation(&mut state, previous)?;
            Ok(team)
        })
    }

    pub fn list_teams(&self, org: &str) -> Result<Vec<Team>> {
        let state = self.runtime.state.read();
        if !state.organizations.contains_key(org) {
            return Err(ForgeError::NotFound(format!("organization {org}")));
        }
        Ok(state
            .teams
            .values()
            .filter(|team| team.organization == org)
            .cloned()
            .collect())
    }
}
