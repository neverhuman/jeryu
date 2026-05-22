use super::*;

/// Pretty-print a sweep report (one provider per line; safe to redirect to a log).
pub fn render_report(results: &[ProviderCheckResult]) -> String {
    let mut s = String::new();
    s.push_str("jeryu autonomy doctor — provider sweep\n");
    s.push_str("──────────────────────────────────────\n");
    for r in results {
        let glyph = match r.status {
            ProviderStatus::Ok => "✓ OK   ",
            ProviderStatus::NoKey => "○ NOKEY",
            ProviderStatus::Auth => "✗ AUTH ",
            ProviderStatus::RateLimited => "△ RATE ",
            ProviderStatus::Unavailable => "✗ DOWN ",
            ProviderStatus::Skipped => "— SKIP ",
        };
        s.push_str(&format!(
            "{glyph}  {:<10}  model={:<60}  {:>5}ms  {}\n",
            r.provider_id, r.model_tried, r.latency_ms, r.note
        ));
    }
    s
}

#[cfg(test)]
mod provider_config_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn probes_are_derived_from_provider_config_in_stable_order() {
        let mut headers = HashMap::new();
        headers.insert("X-Title".to_string(), "jeryu".to_string());
        headers.insert(
            "HTTP-Referer".to_string(),
            "https://example.com".to_string(),
        );
        let config = ProvidersConfig {
            schema: "vibegate.providers.v1".to_string(),
            default_role_chain: vec!["reviewer-security".to_string()],
            chains: HashMap::from([
                (
                    "reviewer-runtime".to_string(),
                    vec![ProviderEntry {
                        provider: "openrouter".to_string(),
                        base_url: "https://openrouter.ai/api/v1".to_string(),
                        model_id: "openai/gpt-oss-120b:free".to_string(),
                        api_key_secret: "OPENROUTER_API_KEY".to_string(),
                        data_use: "no_train".to_string(),
                        temperature: 0.0,
                        timeout_ms: 30_000,
                        max_tokens: 800,
                        extra_headers: HashMap::new(),
                    }],
                ),
                (
                    "reviewer-security".to_string(),
                    vec![ProviderEntry {
                        provider: "openrouter".to_string(),
                        base_url: "https://openrouter.ai/api/v1".to_string(),
                        model_id: "nvidia/nemotron-3-super-120b-a12b:free".to_string(),
                        api_key_secret: "OPENROUTER_API_KEY".to_string(),
                        data_use: "no_train".to_string(),
                        temperature: 0.0,
                        timeout_ms: 30_000,
                        max_tokens: 800,
                        extra_headers: headers,
                    }],
                ),
            ]),
        };

        let probes = DoctorProbe::from_providers_config(&config);

        assert_eq!(probes.len(), 2);
        assert_eq!(probes[0].provider_id, "reviewer-runtime#1:openrouter");
        assert_eq!(probes[1].provider_id, "reviewer-security#1:openrouter");
        assert_eq!(
            probes[1].extra_headers,
            vec![
                (
                    "HTTP-Referer".to_string(),
                    "https://example.com".to_string()
                ),
                ("X-Title".to_string(), "jeryu".to_string()),
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_handles_all_statuses() {
        let results = vec![
            ProviderCheckResult {
                provider_id: "openrouter".into(),
                status: ProviderStatus::Ok,
                model_tried: "x:free".into(),
                latency_ms: 1234,
                note: "ok".into(),
            },
            ProviderCheckResult {
                provider_id: "groq".into(),
                status: ProviderStatus::Auth,
                model_tried: "y".into(),
                latency_ms: 100,
                note: "401".into(),
            },
            ProviderCheckResult {
                provider_id: "cerebras".into(),
                status: ProviderStatus::NoKey,
                model_tried: "z".into(),
                latency_ms: 0,
                note: "no key".into(),
            },
        ];
        let r = render_report(&results);
        assert!(r.contains("OK"));
        assert!(r.contains("AUTH"));
        assert!(r.contains("NOKEY"));
    }
}
