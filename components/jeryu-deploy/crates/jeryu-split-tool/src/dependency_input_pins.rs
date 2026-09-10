use super::*;

impl Inventory {
    pub(super) fn tool_manifest(&mut self, path: &str, manifest: &Value) {
        let pin = &manifest["jankurai"];
        let source = text(&manifest["distribution"], "source_repository");
        let commit = text(pin, "rev");
        let mut facts = json!({"repository":source.and_then(github_slug),"url":source.and_then(safe_url),
            "legacy_locator":text(pin,"repo").and_then(safe_url),"commit":commit,
            "tag":pin.get("tag"),"version":pin.get("version"),"license":null,
            "required_for_application_install":false});
        for field in [
            "source_tree",
            "source_archive_sha256",
            "cargo_lock_sha256",
            "binary_sha256",
            "vendor_files_sha256",
            "build_context_sha256",
            "builder_image",
            "rust_toolchain",
            "package_path",
        ] {
            facts[field] = pin[field].clone();
            if field.ends_with("sha256") && !text(pin, field).is_some_and(digest) {
                self.error(
                    path,
                    &format!("jankurai.{field}"),
                    "invalid_pinned_digest",
                    &[AUDIT],
                );
            }
        }
        if !commit.is_some_and(oid) || source.and_then(github_slug).is_none() {
            self.error(
                path,
                "jankurai",
                "auditor_source_identity_unresolved",
                &[AUDIT],
            );
        }
        self.observe(
            path,
            "jankurai",
            "auditor-pin",
            "full repository audit",
            &[AUDIT],
            facts,
        );
        self.gap(
            path,
            "jankurai",
            "external_producer_dependency_closure_not_loaded",
            &[AUDIT],
        );
        if let Some(floors) = manifest.get("floors").and_then(Value::as_object) {
            for (profile, score) in floors {
                let required_floor = crate::audit_census::effective_floor(profile);
                self.observe(
                    path,
                    &format!("floors.{profile}"),
                    "audit-profile",
                    profile,
                    &[AUDIT],
                    json!({"declared_minimum":score,"required_minimum":required_floor}),
                );
                if !score
                    .as_u64()
                    .is_some_and(|value| value >= u64::from(required_floor) && value <= 100)
                {
                    self.error(
                        path,
                        &format!("floors.{profile}"),
                        "audit_profile_below_effective_floor",
                        &[AUDIT],
                    );
                }
            }
        } else {
            self.error(path, "floors", "missing_audit_profiles", &[AUDIT]);
        }
    }
    pub(super) fn tsv(&mut self, path: &str, source: &str, artifacts: bool) {
        for (index, line) in source.lines().enumerate() {
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let fields: Vec<_> = line.split('\t').collect();
            let location = format!("line:{}", index + 1);
            if fields.len() != if artifacts { 5 } else { 2 }
                || fields.iter().any(|field| field.is_empty())
            {
                self.error(path, &location, "invalid_tool_lock_row", &[AUDIT, MIRRORS]);
                continue;
            }
            let facts = if artifacts {
                if !digest(fields[3]) {
                    self.error(
                        path,
                        &location,
                        "invalid_tool_artifact_digest",
                        &[AUDIT, MIRRORS],
                    );
                }
                json!({"package":fields[0],"version":fields[1],"archive_member":fields[2],
                    "sha256":fields[3],"url":safe_url(fields[4]),"repository":github_slug(fields[4]),
                    "commit":null,"license":null})
            } else {
                json!({"package":fields[0],"version":fields[1],"source_kind":"registry",
                "repository":null,"commit":null,"license":null})
            };
            self.observe(
                path,
                &location,
                "ci-tool-lock",
                "CI prerequisite",
                &[AUDIT, MIRRORS],
                facts,
            );
        }
    }
}
