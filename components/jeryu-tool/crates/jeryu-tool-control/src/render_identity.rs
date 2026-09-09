pub(super) const CANONICAL_FAMILY_ROOT: &str = "/home/ubuntu/jain-split/jeryu-split";
pub(super) const GIT_BIN: &str = "/usr/bin/git";
pub(super) const SHA256_BIN: &str = "/usr/bin/sha256sum";
pub(super) const HOSTED_JERYU_GIT_BASE: &str = "https://git.neverhuman.org/git/jeryu";
pub(super) const DIRECT_HOST_NO_PROXY: &str = "git.neverhuman.org,127.0.0.1,localhost,::1";
pub(super) const SCRUBBED_GIT_ENVIRONMENT: &[&str] = &[
    // Git repository, configuration, transport, and credential overrides.
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_ASKPASS",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_DIR",
    "GIT_EXEC_PATH",
    "GIT_EXTERNAL_DIFF",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PROXY_COMMAND",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "GIT_WORK_TREE",
    "SSH_ASKPASS",
    // Ambient proxies must not redirect the credential-bearing HTTPS request.
    "ALL_PROXY",
    "all_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "NO_PROXY",
    "no_proxy",
    // TLS identity, verification, and key-log overrides.
    "CURL_CA_BUNDLE",
    "CURL_SSL_BACKEND",
    "GIT_SSL_CAINFO",
    "GIT_SSL_CAPATH",
    "GIT_SSL_CERT",
    "GIT_SSL_CERT_PASSWORD_PROTECTED",
    "GIT_SSL_CIPHER_LIST",
    "GIT_SSL_KEY",
    "GIT_SSL_NO_VERIFY",
    "GIT_SSL_VERSION",
    "GNUTLS_CPUID_OVERRIDE",
    "GNUTLS_DEBUG_LEVEL",
    "GNUTLS_NO_IMPLICIT_INIT",
    "GNUTLS_SYSTEM_PRIORITY_FILE",
    "NSS_SSLKEYLOGFILE",
    "OPENSSL_CONF",
    "OPENSSL_CONF_INCLUDE",
    "OPENSSL_ENGINES",
    "OPENSSL_MODULES",
    "SSL_CERT_DIR",
    "SSL_CERT_FILE",
    "SSLKEYLOGFILE",
    // Git trace and standard-stream redirection sinks.
    "GIT_CURL_VERBOSE",
    "GIT_REDIRECT_STDERR",
    "GIT_REDIRECT_STDIN",
    "GIT_REDIRECT_STDOUT",
    "GIT_TRACE",
    "GIT_TRACE_CURL",
    "GIT_TRACE_CURL_NO_DATA",
    "GIT_TRACE_PACKET",
    "GIT_TRACE_PACK_ACCESS",
    "GIT_TRACE_PACKFILE",
    "GIT_TRACE_PERFORMANCE",
    "GIT_TRACE_REDACT",
    "GIT_TRACE_SETUP",
    "GIT_TRACE_SHALLOW",
    "GIT_TRACE2",
    "GIT_TRACE2_BRIEF",
    "GIT_TRACE2_CONFIG_PARAMS",
    "GIT_TRACE2_DST_DEBUG",
    "GIT_TRACE2_ENV_VARS",
    "GIT_TRACE2_EVENT",
    "GIT_TRACE2_EVENT_BRIEF",
    "GIT_TRACE2_EVENT_NESTING",
    "GIT_TRACE2_MAX_FILES",
    "GIT_TRACE2_PARENT_NAME",
    "GIT_TRACE2_PARENT_SID",
    "GIT_TRACE2_PERF",
    "GIT_TRACE2_PERF_BRIEF",
    // Dynamic-loader and locale module injection into the fixed Git binary.
    "GCONV_PATH",
    "GLIBC_TUNABLES",
    "LD_AUDIT",
    "LD_DEBUG",
    "LD_DEBUG_OUTPUT",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "LD_PROFILE",
    "LD_PROFILE_OUTPUT",
    "LOCPATH",
];
pub(super) const CANONICAL_REPOS: [&str; 11] = [
    "jeryu",
    "jeryu-cache",
    "jeryu-ci-runner",
    "jeryu-core",
    "jeryu-deploy",
    "jeryu-intelligence",
    "jeryu-jira",
    "jeryu-release-ops",
    "jeryu-tool",
    "jeryu-tool-finder",
    "jeryu-web",
];

pub(super) fn canonical_hosted_origin(name: &str) -> Result<String, String> {
    if !CANONICAL_REPOS.contains(&name) {
        return Err(format!(
            "renderer has no canonical hosted origin for repository: {name:?}"
        ));
    }
    Ok(format!("{HOSTED_JERYU_GIT_BASE}/{name}.git"))
}

pub(super) fn require_canonical_hosted_origin(name: &str, origin: &str) -> Result<String, String> {
    let expected = canonical_hosted_origin(name)?;
    if origin != expected {
        return Err(format!(
            "renderer repository has non-canonical hosted origin: {name}={origin}; expected {expected}"
        ));
    }
    Ok(expected)
}
