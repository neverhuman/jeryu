use jitforge_enterprise::{OidcConfig, SamlConfig, SsoError};

#[test]
fn oidc_requires_https_redirect() {
    let config = OidcConfig {
        issuer: "https://idp.example".to_owned(),
        audience: "jitforge".to_owned(),
        redirect_uri: "http://forge.example/callback".to_owned(),
        jwks_uri: "https://idp.example/jwks".to_owned(),
    };
    assert_eq!(config.validate(), Err(SsoError::InsecureRedirectUri));
}

#[test]
fn saml_requires_certificate() {
    let config = SamlConfig {
        entity_id: "jitforge".to_owned(),
        sso_url: "https://idp.example/saml".to_owned(),
        certificate_pem: "missing".to_owned(),
    };
    assert_eq!(config.validate(), Err(SsoError::MissingSamlCertificate));
}
