#![doc = "The host allowlist and its matching rules."]

/// The default set of exact hostnames every cell is permitted to reach.
///
/// These cover the Anthropic / OpenAI model APIs and the crates.io + GitHub
/// fetch paths that a Rust build needs. Everything else is denied.
pub const DEFAULT_HOSTS: &[&str] = &[
    "api.anthropic.com",
    "api.openai.com",
    "crates.io",
    "static.crates.io",
    "index.crates.io",
    "github.com",
    "codeload.github.com",
    "objects.githubusercontent.com",
];

/// The default set of allowed DNS suffixes (subdomain wildcards).
///
/// A suffix entry like `.crates.io` permits any subdomain of `crates.io`
/// (e.g. `foo.crates.io`) as well as the apex `crates.io` itself, but NEVER an
/// unrelated host that merely *contains* the string (e.g.
/// `crates.io.attacker.com`).
pub const DEFAULT_SUFFIXES: &[&str] = &[".crates.io"];

/// An immutable host allowlist used by [`crate::egress_decision`].
///
/// Hosts are stored pre-lowercased so matching is case-insensitive and
/// allocation-free at decision time (the candidate is lowercased once per call).
#[derive(Debug, Clone)]
pub struct Allowlist {
    /// Exact hostnames, lowercased.
    hosts: Vec<String>,
    /// Allowed suffixes, lowercased, each guaranteed to start with `.`.
    suffixes: Vec<String>,
}

impl Default for Allowlist {
    fn default() -> Self {
        Self::new(
            DEFAULT_HOSTS.iter().map(|h| (*h).to_string()),
            DEFAULT_SUFFIXES.iter().map(|s| (*s).to_string()),
        )
    }
}

impl Allowlist {
    /// Build an allowlist from exact hosts and suffix rules.
    ///
    /// Inputs are normalised to lowercase. Any suffix that does not already begin
    /// with `.` has one prepended, so callers may pass either `crates.io` or
    /// `.crates.io` and get the same (safe, dot-boundary) semantics.
    pub fn new<H, S>(hosts: H, suffixes: S) -> Self
    where
        H: IntoIterator<Item = String>,
        S: IntoIterator<Item = String>,
    {
        let hosts = hosts
            .into_iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| !h.is_empty())
            .collect();
        let suffixes = suffixes
            .into_iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty() && s != ".")
            .map(|s| {
                if s.starts_with('.') {
                    s
                } else {
                    format!(".{s}")
                }
            })
            .collect();
        Self { hosts, suffixes }
    }

    /// Whether `host` is permitted by this allowlist.
    ///
    /// Matching is exact on the host set, OR a true DNS-suffix match against the
    /// suffix set. A suffix `.crates.io` matches `crates.io` (apex) and
    /// `foo.crates.io` (subdomain) but NOT `crates.io.attacker.com` or
    /// `evilcrates.io`. This is deliberately NOT `str::contains`.
    #[must_use]
    pub fn permits(&self, host: &str) -> bool {
        // Normalise: lowercase, strip any trailing dot (FQDN form) and a possible
        // :port that a malformed authority might carry.
        let host = host.trim().to_ascii_lowercase();
        let host = host.split(':').next().unwrap_or(&host);
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty() {
            return false;
        }

        if self.hosts.iter().any(|h| h == host) {
            return true;
        }

        self.suffixes
            .iter()
            .any(|suffix| matches_suffix(host, suffix))
    }

    /// The exact hosts on this allowlist (lowercased).
    #[must_use]
    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }

    /// The suffix rules on this allowlist (lowercased, dot-prefixed).
    #[must_use]
    pub fn suffixes(&self) -> &[String] {
        &self.suffixes
    }
}

/// True DNS-suffix match. `suffix` is guaranteed to start with `.`.
///
/// `host` matches when it equals the apex (`suffix` without the leading dot) or
/// when it ends with `suffix` on a label boundary. This rejects substring
/// attacks (`crates.io.attacker.com`) and missing-dot prefixes (`evilcrates.io`).
fn matches_suffix(host: &str, suffix: &str) -> bool {
    let apex = &suffix[1..]; // suffix always starts with '.'
    host == apex || host.ends_with(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apex_matches_suffix_rule() {
        let allow = Allowlist::default();
        assert!(allow.permits("crates.io"));
    }

    #[test]
    fn subdomain_matches_suffix_rule() {
        let allow = Allowlist::default();
        assert!(allow.permits("foo.crates.io"));
        assert!(allow.permits("a.b.crates.io"));
    }

    #[test]
    fn substring_attack_rejected() {
        let allow = Allowlist::default();
        assert!(!allow.permits("crates.io.attacker.com"));
        assert!(!allow.permits("evilcrates.io"));
    }

    #[test]
    fn trailing_dot_and_port_are_normalised() {
        let allow = Allowlist::default();
        assert!(allow.permits("github.com."));
        assert!(allow.permits("github.com:443"));
        assert!(allow.permits("foo.crates.io.:443"));
    }

    #[test]
    fn custom_suffix_without_dot_is_normalised() {
        let allow = Allowlist::new(
            std::iter::empty(),
            std::iter::once("example.com".to_string()),
        );
        assert!(allow.permits("example.com"));
        assert!(allow.permits("a.example.com"));
        assert!(!allow.permits("notexample.com"));
        assert!(!allow.permits("example.com.evil.net"));
    }

    #[test]
    fn empty_host_is_denied() {
        let allow = Allowlist::default();
        assert!(!allow.permits(""));
        assert!(!allow.permits("   "));
    }
}
