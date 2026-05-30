//! SSO validation for OIDC and SAML.

/// SSO validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SsoError {
    MissingIssuer,
    MissingAudience,
    InsecureRedirectUri,
    MissingJwksUri,
    MissingSamlCertificate,
    MissingEntityId,
}

/// OIDC configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OidcConfig {
    pub issuer: String,
    pub audience: String,
    pub redirect_uri: String,
    pub jwks_uri: String,
}

impl OidcConfig {
    /// Validate fail-closed OIDC config.
    pub fn validate(&self) -> Result<(), SsoError> {
        if self.issuer.trim().is_empty() {
            return Err(SsoError::MissingIssuer);
        }
        if self.audience.trim().is_empty() {
            return Err(SsoError::MissingAudience);
        }
        if !self.redirect_uri.starts_with("https://") {
            return Err(SsoError::InsecureRedirectUri);
        }
        if self.jwks_uri.trim().is_empty() {
            return Err(SsoError::MissingJwksUri);
        }
        Ok(())
    }
}

/// SAML configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamlConfig {
    pub entity_id: String,
    pub sso_url: String,
    pub certificate_pem: String,
}

impl SamlConfig {
    /// Validate fail-closed SAML config.
    pub fn validate(&self) -> Result<(), SsoError> {
        if self.entity_id.trim().is_empty() {
            return Err(SsoError::MissingEntityId);
        }
        if self.sso_url.trim().is_empty() {
            return Err(SsoError::MissingIssuer);
        }
        if !self.certificate_pem.contains("BEGIN CERTIFICATE") {
            return Err(SsoError::MissingSamlCertificate);
        }
        Ok(())
    }
}
