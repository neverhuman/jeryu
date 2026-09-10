use super::*;

impl Inventory {
    pub(super) fn bind_cargo_declarations(&mut self) {
        for (declaration_index, declaration) in self.observations.clone().into_iter().enumerate() {
            if text(&declaration, "kind") != Some("cargo-declaration") {
                continue;
            }
            let facts = &declaration["facts"];
            let Some(repository) =
                text(facts, "repository").filter(|repo| repo.starts_with("neverhuman/"))
            else {
                continue;
            };
            let commit = text(facts, "commit");
            let tag = text(facts, "tag_selector");
            let package = text(facts, "package");
            let candidates: Vec<_> = self
                .observations
                .iter()
                .enumerate()
                .filter(|(_, locked)| {
                    text(locked, "kind") == Some("cargo-locked-package")
                        && text(locked, "path") == text(facts, "owning_lock")
                        && text(&locked["facts"], "package") == package
                        && text(&locked["facts"], "repository") == Some(repository)
                        && text(facts, "url").is_some()
                        && text(&locked["facts"], "url") == text(facts, "url")
                        && (commit.is_some() || tag.is_some())
                        && commit
                            .is_none_or(|value| text(&locked["facts"], "commit") == Some(value))
                        && tag.is_none_or(|value| {
                            text(&locked["facts"], "tag_selector") == Some(value)
                        })
                })
                .map(|(index, _)| index)
                .collect();
            let caps: Vec<_> = declaration["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect();
            if candidates.len() != 1 {
                self.error(
                    declaration["path"].as_str().unwrap(),
                    declaration["location"].as_str().unwrap(),
                    "git_declaration_not_bound_to_owning_lock_package",
                    &caps,
                );
                continue;
            }
            let locked_index = candidates[0];
            let mut inherited: BTreeSet<String> = self.observations[locked_index]["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            inherited.extend(caps.into_iter().map(str::to_owned));
            self.observations[locked_index]["capabilities"] = json!(inherited);
            let locked = &self.observations[locked_index];
            let binding = json!({"path":locked["path"],"location":locked["location"],
                "package":locked["facts"]["package"],"version":locked["facts"]["version"],
                "commit":locked["facts"]["commit"],"url":locked["facts"]["url"]});
            self.observations[declaration_index]["facts"]["lock_binding"] = binding;
        }
    }

    pub(super) fn compare_enrollment(&mut self, inventory: &Value) {
        let path = "agent/audit-repositories.json";
        let Ok(version) =
            crate::audit_census::enrollment::version(text(inventory, "schema").unwrap_or(""))
        else {
            self.error(path, "schema", "invalid_audit_enrollment_schema", &[AUDIT]);
            return;
        };
        let Some(sources) = inventory.get("sources").and_then(Value::as_array) else {
            self.error(
                path,
                "sources",
                "missing_audit_enrollment_sources",
                &[AUDIT],
            );
            return;
        };
        self.bind_cargo_declarations();
        let mut identities = BTreeSet::new();
        let mut valid = BTreeSet::new();
        for (index, source) in sources.iter().enumerate() {
            let key = serde_json::from_value::<crate::audit_census::Source>(source.clone())
                .ok()
                .and_then(|row| crate::audit_census::enrollment::identity(&row, version).ok());
            if !source.is_object() || !key.is_some_and(|key| identities.insert(key)) {
                self.error(
                    path,
                    &format!("sources[{index}]"),
                    "invalid_or_duplicate_enrollment",
                    &[AUDIT],
                );
            } else {
                valid.insert(index);
            }
        }
        let mut used = BTreeSet::new();
        let observations = self.observations.clone();
        for observation in observations {
            let facts = &observation["facts"];
            let Some(repo) =
                text(facts, "repository").filter(|repo| repo.starts_with("neverhuman/"))
            else {
                continue;
            };
            if text(&observation, "kind") == Some("family-repository") {
                continue;
            }
            let caps: Vec<_> = observation["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let source_path = observation["path"].as_str().unwrap();
            let location = observation["location"].as_str().unwrap();
            let commit = text(facts, "commit").filter(|value| oid(value));
            if text(&observation, "kind") == Some("cargo-declaration") {
                continue;
            }
            let floor = self.floors.get(repo).copied().unwrap_or_else(|| {
                crate::audit_census::effective_floor(repo.rsplit('/').next().unwrap_or(repo))
            });
            let matching: Vec<_> = sources
                .iter()
                .enumerate()
                .filter(|(index, row)| {
                    valid.contains(index)
                        && row["path"].is_null()
                        && matches!(text(row, "scope"), Some("dependency" | "optional"))
                        && text(row, "repository") == Some(repo)
                        && text(row, "commit") == commit
                        && commit.is_some()
                })
                .collect();
            let status = if commit.is_none() {
                "immutable_identity_missing"
            } else if matching.is_empty() {
                "missing_or_wrong_revision"
            } else if !matching.iter().any(|(_, row)| {
                (!required(&caps)
                    || row["required"] == true && text(row, "scope") == Some("dependency"))
                    && row["minimum"]
                        .as_u64()
                        .is_some_and(|score| score >= u64::from(floor) && score <= 100)
            }) {
                "requiredness_or_floor_mismatch"
            } else {
                "enrolled"
            };
            for (index, _) in matching {
                used.insert(index);
            }
            self.enrollment.push(
                json!({"path":source_path,"location":location,"repository":repo,
                "commit":commit,"capabilities":caps,"minimum":floor,"status":status}),
            );
            if status != "enrolled" {
                if required(&caps) {
                    self.error(source_path, location, status, &caps);
                } else {
                    self.gap(source_path, location, status, &caps);
                }
            }
        }
        for (index, source) in sources
            .iter()
            .enumerate()
            .filter(|(index, _)| !used.contains(index))
        {
            self.enrollment
                .push(json!({"path":path,"location":format!("sources[{index}]"),
                "repository":source.get("repository"),"commit":source.get("commit"),
                "required":source.get("required"),"status":"declared_unconsumed"}));
            if source["required"] == true {
                self.gap(
                    path,
                    &format!("sources[{index}]"),
                    "required_enrollment_without_source_observation",
                    &[AUDIT, MIRRORS],
                );
            }
        }
    }
    pub(super) fn finish(mut self) -> Result<Value> {
        for rows in [
            &mut self.observations,
            &mut self.unresolved,
            &mut self.enrollment,
            &mut self.errors,
        ] {
            for row in rows.iter_mut() {
                row.sort_all_objects();
            }
            rows.sort_by_cached_key(Value::to_string);
            rows.dedup();
        }
        let inputs: Vec<_> = self.inputs.into_values().collect();
        let identity = hash(
            crate::canonical_json::pretty(json!({"schema":"jeryu.dependency-inputs/v1",
            "parser_version":1,"inputs":inputs}))?
            .as_bytes(),
        );
        let mut complete = serde_json::Map::new();
        for cap in ALL {
            let count = self
                .unresolved
                .iter()
                .filter(|gap| {
                    gap["capabilities"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|value| value.as_str() == Some(*cap))
                })
                .count();
            complete.insert(
                (*cap).into(),
                json!({"complete":count==0,"unresolved_count":count}),
            );
        }
        Ok(
            json!({"schema":"jeryu.dependency-inputs/v1","source_input_sha256":identity,
            "source_binding":"caller must bind this body hash to exact commit/tree; no availability or audit claim",
            "inputs":inputs,"observations":self.observations,"unresolved":self.unresolved,
            "enrollment":self.enrollment,"complete_for":complete,"errors":self.errors}),
        )
    }
}
