//! Users, organizations, and teams.

use chrono::Utc;
use uuid::Uuid;

use super::{ForgeCore, require_name, slugify};
use crate::errors::{ForgeError, Result};
use crate::model::*;

impl ForgeCore {
    pub fn create_user(&self, request: CreateUserRequest) -> Result<User> {
        require_name("login", &request.login)?;
        let mut state = self.state.write();
        if state.users.contains_key(&request.login) {
            return Err(ForgeError::Conflict(format!("user {}", request.login)));
        }
        let user = User {
            id: Uuid::new_v4(),
            login: request.login.clone(),
            name: request.name,
            email: request.email,
            created_at: Utc::now(),
        };
        state.users.insert(request.login, user.clone());
        Ok(user)
    }

    pub fn get_user(&self, login: &str) -> Result<User> {
        self.state
            .read()
            .users
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("user {login}")))
    }

    pub fn ensure_user(&self, login: &str) -> User {
        if let Ok(user) = self.get_user(login) {
            return user;
        }
        self.create_user(CreateUserRequest {
            login: login.to_string(),
            name: None,
            email: None,
        })
        .expect("ensure_user creates a unique user after not-found")
    }

    pub fn create_organization(&self, request: CreateOrganizationRequest) -> Result<Organization> {
        require_name("login", &request.login)?;
        let mut state = self.state.write();
        if state.organizations.contains_key(&request.login) {
            return Err(ForgeError::Conflict(format!(
                "organization {}",
                request.login
            )));
        }
        let organization = Organization {
            id: Uuid::new_v4(),
            login: request.login.clone(),
            display_name: request.display_name,
            created_at: Utc::now(),
        };
        state
            .organizations
            .insert(request.login, organization.clone());
        Ok(organization)
    }

    pub fn get_organization(&self, login: &str) -> Result<Organization> {
        self.state
            .read()
            .organizations
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("organization {login}")))
    }

    pub fn create_team(&self, org: &str, request: CreateTeamRequest) -> Result<Team> {
        require_name("team name", &request.name)?;
        let slug = request.slug.unwrap_or_else(|| slugify(&request.name));
        require_name("team slug", &slug)?;
        let mut state = self.state.write();
        if !state.organizations.contains_key(org) {
            return Err(ForgeError::NotFound(format!("organization {org}")));
        }
        let key = (org.to_string(), slug.clone());
        if state.teams.contains_key(&key) {
            return Err(ForgeError::Conflict(format!("team {org}/{slug}")));
        }
        let team = Team {
            id: Uuid::new_v4(),
            organization: org.to_string(),
            slug: slug.clone(),
            name: request.name,
            members: request.members,
            created_at: Utc::now(),
        };
        state.teams.insert(key, team.clone());
        Ok(team)
    }

    pub fn list_teams(&self, org: &str) -> Result<Vec<Team>> {
        let state = self.state.read();
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
