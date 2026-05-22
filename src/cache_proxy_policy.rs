#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ProxyVerdict {
    MitmIntercept,
    Passthrough,
}

impl ProxyVerdict {
    pub(crate) fn classify(host: &str) -> Self {
        if host.contains("crates.io") || host.contains("npmjs.org") {
            Self::MitmIntercept
        } else {
            Self::Passthrough
        }
    }

    pub(crate) fn reason_code(&self) -> &'static str {
        match self {
            Self::MitmIntercept => "intercepted_passthrough",
            Self::Passthrough => "passthrough",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProxyVerdict;

    #[test]
    fn test_proxy_verdict_classification() {
        assert_eq!(
            ProxyVerdict::classify("crates.io:443"),
            ProxyVerdict::MitmIntercept
        );
        assert_eq!(
            ProxyVerdict::classify("registry.npmjs.org:443"),
            ProxyVerdict::MitmIntercept
        );
        assert_eq!(
            ProxyVerdict::classify("github.com:443"),
            ProxyVerdict::Passthrough
        );
        assert_eq!(
            ProxyVerdict::classify("api.stripe.com"),
            ProxyVerdict::Passthrough
        );
    }

    #[test]
    fn test_proxy_verdict_reasons() {
        assert_eq!(
            ProxyVerdict::MitmIntercept.reason_code(),
            "intercepted_passthrough"
        );
        assert_eq!(ProxyVerdict::Passthrough.reason_code(), "passthrough");
    }
}
