use super::*;

impl Inventory {
    pub(super) fn cargo_lock(&mut self, path: &str, lock: &Value, caps: &[&str]) {
        let Some(packages) = lock.get("package").and_then(Value::as_array) else {
            self.error(path, "package", "missing_lock_packages", caps);
            return;
        };
        let mut seen = BTreeSet::new();
        for (index, package) in packages.iter().enumerate() {
            let location = format!("package[{index}]");
            let (Some(name), Some(version)) = (text(package, "name"), text(package, "version"))
            else {
                self.error(path, &location, "invalid_locked_package", caps);
                continue;
            };
            let source = text(package, "source");
            if !seen.insert((name, version, source)) {
                self.error(path, &location, "duplicate_locked_package", caps);
            }
            let mut facts = json!({"package":name,"version":version,
                "checksum_sha256":package.get("checksum"),"license":null,
                "source_kind":if source.is_none() {"workspace"} else {"registry"},
                "dependency_edges":package.get("dependencies"),"repository":null,"commit":null});
            if let Some(source) = source {
                if let Some(git) = source.strip_prefix("git+") {
                    let (selector, commit) = git.rsplit_once('#').unwrap_or((git, ""));
                    let (url, query) = selector.split_once('?').unwrap_or((selector, ""));
                    facts["source_kind"] = json!("git");
                    facts["url"] = json!(safe_url(url));
                    facts["repository"] = json!(github_slug(url));
                    facts["commit"] = json!(oid(commit).then_some(commit));
                    facts["tag_selector"] =
                        json!(query.split('&').find_map(|part| part.strip_prefix("tag=")));
                    if !oid(commit) {
                        self.gap(path, &location, "git_commit_not_immutable", caps);
                    }
                    if safe_url(url).is_none() {
                        self.gap(
                            path,
                            &location,
                            "unsupported_or_sensitive_git_locator",
                            caps,
                        );
                    }
                } else if source.starts_with("registry+") {
                    if !text(package, "checksum").is_some_and(digest) {
                        self.error(
                            path,
                            &location,
                            "registry_checksum_missing_or_invalid",
                            caps,
                        );
                    }
                } else {
                    self.gap(path, &location, "unsupported_cargo_source", caps);
                }
            }
            self.observe(
                path,
                &location,
                "cargo-locked-package",
                "workspace-resolution",
                caps,
                facts,
            );
        }
    }
    pub(super) fn cargo_manifest(
        &mut self,
        path: &str,
        lock_path: &str,
        manifest: &Value,
        workspace: &Value,
        caps: &[&str],
    ) {
        let package = &manifest["package"];
        let name = text(package, "name").unwrap_or("workspace");
        let inherited = |field: &str| {
            let value = &package[field];
            if value.get("workspace") == Some(&json!(true)) {
                workspace["package"][field].clone()
            } else {
                value.clone()
            }
        };
        self.observe(path, "package", "cargo-manifest", name, caps,
            json!({"package":name,"version":inherited("version"),"license":inherited("license"),
                "repository_url":inherited("repository").as_str().and_then(safe_url),"feature_resolution":"not-executed"}));
        let mut groups = vec![("".to_string(), manifest)];
        if let Some(targets) = manifest.get("target").and_then(Value::as_object) {
            groups.extend(
                targets
                    .iter()
                    .map(|(target, value)| (format!("target.{target}."), value)),
            );
        }
        for (prefix, group) in groups {
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                let Some(dependencies) = group.get(kind).and_then(Value::as_object) else {
                    continue;
                };
                for (dependency, original) in dependencies {
                    let resolved = if original.get("workspace") == Some(&json!(true)) {
                        &workspace["dependencies"][dependency]
                    } else {
                        original
                    };
                    let location = format!("{prefix}{kind}.{dependency}");
                    if resolved.is_null() {
                        self.error(path, &location, "missing_workspace_dependency", caps);
                    }
                    let git = text(resolved, "git");
                    let commit = text(resolved, "rev").filter(|rev| oid(rev));
                    self.observe(path, &location, "cargo-declaration", name, caps,
                        json!({"dependency":dependency,"package":text(resolved,"package").unwrap_or(dependency),"owning_lock":lock_path,
                            "version":resolved.as_str().or_else(|| text(resolved,"version")),
                            "local_path":resolved.get("path"),"optional":original.get("optional").or_else(||resolved.get("optional")),
                            "repository":git.and_then(github_slug),"url":git.and_then(safe_url),
                            "commit":commit,"tag_selector":resolved.get("tag"),
                            "condition":format!("{prefix}{kind}")}));
                }
            }
        }
    }
    pub(super) fn npm_lock(
        &mut self,
        path: &str,
        lock: &Value,
        local: &BTreeSet<String>,
        caps: &[&str],
    ) {
        if !matches!(
            lock.get("lockfileVersion").and_then(Value::as_u64),
            Some(2 | 3)
        ) {
            self.error(
                path,
                "lockfileVersion",
                "unsupported_npm_lock_version",
                caps,
            );
            return;
        }
        let Some(packages) = lock.get("packages").and_then(Value::as_object) else {
            self.error(path, "packages", "missing_npm_packages", caps);
            return;
        };
        for (package_path, package) in packages {
            let location = format!("packages[{package_path}]");
            let linked = package.get("link") == Some(&json!(true));
            let resolved = text(package, "resolved");
            let local_target =
                linked && resolved.is_some_and(|target| relative(target) && local.contains(target));
            let workspace = package_path.is_empty() || local.contains(package_path) || local_target;
            if linked && !local_target {
                self.gap(path, &location, "unresolved_npm_link", caps);
            }
            let integrity = text(package, "integrity");
            let git = resolved.and_then(|value| value.strip_prefix("git+"));
            let git_parts = git.map(|value| value.rsplit_once('#').unwrap_or((value, "")));
            let git_url = git_parts.map(|(url, _)| url.split('?').next().unwrap_or(url));
            let commit = git_parts
                .map(|(_, commit)| commit)
                .filter(|commit| oid(commit));
            if git.is_some() && commit.is_none() {
                self.gap(path, &location, "npm_git_commit_not_immutable", caps);
            }
            if git.is_some() && git_url.and_then(github_slug).is_none() {
                self.gap(path, &location, "npm_git_repository_not_resolved", caps);
            }
            if !workspace && !linked && git.is_none() && integrity.is_none() {
                self.gap(path, &location, "npm_integrity_missing", caps);
            }
            self.observe(path, &location, "npm-locked-package", package_path, caps,
                json!({"package":package.get("name"),"version":package.get("version"),
                    "source_kind":if workspace {"workspace"} else if git.is_some() {"git"} else {"package"},
                    "workspace_target":if local_target {resolved} else {None},
                    "url":git_url.or(resolved).and_then(safe_url),"integrity":integrity,
                    "license":package.get("license"),"repository":git_url.and_then(github_slug),"commit":commit,
                    "dependency_edges":package.get("dependencies"),"optional":package.get("optional"),
                    "os":package.get("os"),"cpu":package.get("cpu")}));
        }
    }
}
