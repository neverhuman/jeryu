use super::*;

pub(super) fn generate(root: &Path) -> Result<Value> {
    let root = fs::canonicalize(root).context("canonical source root")?;
    let root = &root;
    let mut report = Inventory::default();
    let cargo = report.structured(root, "Cargo.toml", "toml", REQUIRED);
    if let Some(lock) = report.structured(root, "Cargo.lock", "toml", REQUIRED) {
        report.cargo_lock("Cargo.lock", &lock, REQUIRED);
    }
    if let Some(cargo) = cargo {
        let workspace = &cargo["workspace"];
        report.cargo_manifest("Cargo.toml", "Cargo.lock", &cargo, workspace, REQUIRED);
        for member in member_paths(&mut report, "Cargo.toml", &workspace["members"], REQUIRED) {
            let path = format!("{member}/Cargo.toml");
            if let Some(manifest) = report.structured(root, &path, "toml", REQUIRED) {
                report.cargo_manifest(&path, "Cargo.lock", &manifest, workspace, REQUIRED);
            }
        }
    }
    report.gap(
        "Cargo.toml",
        "workspace",
        "selected_install_feature_resolution_not_executed",
        &[APP],
    );
    let npm = report.structured(root, "package.json", "json", REQUIRED);
    let mut npm_members = BTreeSet::new();
    if let Some(npm) = npm {
        let members = if npm["workspaces"].is_object() {
            &npm["workspaces"]["packages"]
        } else {
            &npm["workspaces"]
        };
        npm_members.extend(member_paths(&mut report, "package.json", members, REQUIRED));
        report.observe(
            "package.json",
            "scripts",
            "npm-entrypoints",
            "npm workspace",
            REQUIRED,
            json!({"package":npm.get("name"),"license":npm.get("license")}),
        );
        report.gap(
            "package.json",
            "scripts",
            "npm_script_execution_graph_unresolved",
            REQUIRED,
        );
        for member in &npm_members {
            let path = format!("{member}/package.json");
            if let Some(package) = report.structured(root, &path, "json", REQUIRED) {
                report.observe(
                    &path,
                    "package",
                    "npm-workspace",
                    member,
                    REQUIRED,
                    json!({"package":package.get("name"),"version":package.get("version"),
                        "license":package.get("license"),"source_kind":"workspace"}),
                );
            }
        }
    }
    if let Some(lock) = report.structured(root, "package-lock.json", "json", REQUIRED) {
        report.npm_lock("package-lock.json", &lock, &npm_members, REQUIRED);
    }
    let tool = "components/jeryu-tool/tool-manifest.toml";
    if let Some(manifest) = report.structured(root, tool, "toml", &[AUDIT]) {
        report.tool_manifest(tool, &manifest);
    }
    for path in [
        "ci/tools.lock.tsv",
        "ci/cargo-tools.lock.tsv",
        "ci/predecessor-cargo-tools.lock.tsv",
    ] {
        if let Some(source) = report.load(root, path, "tool-lock-tsv", &[AUDIT, MIRRORS]) {
            report.tsv(path, &source, path == "ci/tools.lock.tsv");
        }
    }
    let mut components = vec![("jeryu".to_string(), String::new())];
    if let Some(manifest) = report.structured(
        root,
        "repos.manifest.toml",
        "toml",
        &[AUDIT, MIRRORS, REDLINE],
    ) {
        if let Some(repos) = manifest["repo"].as_array() {
            for repo in repos {
                let (Some(name), Some(path), Some(slug)) = (
                    text(repo, "name"),
                    text(repo, "path"),
                    text(repo, "github_slug"),
                ) else {
                    report.error(
                        "repos.manifest.toml",
                        "repo",
                        "invalid_family_repository",
                        &[AUDIT, MIRRORS],
                    );
                    continue;
                };
                if path != "." && (path != format!("components/{name}") || !relative(path)) {
                    report.error(
                        "repos.manifest.toml",
                        "repo.path",
                        "noncanonical_component_path",
                        &[AUDIT, MIRRORS],
                    );
                    continue;
                }
                let minimum = crate::audit_census::effective_floor(name);
                report.floors.insert(slug.into(), minimum);
                report.observe("repos.manifest.toml",&format!("repo.{name}"),"family-repository",name,&[AUDIT,MIRRORS],
                    json!({"repository":slug,"tag":repo.get("current_tag"),"predecessor_tag":repo.get("predecessor_tag"),
                        "minimum":minimum,"source_path":path}));
                if path != "." {
                    components.push((name.into(), path.into()));
                }
            }
        } else {
            report.error(
                "repos.manifest.toml",
                "repo",
                "missing_family_repository_inventory",
                &[AUDIT, MIRRORS],
            );
        }
        let redline = &manifest["redline"];
        if redline["required_for_release"] != false {
            report.error(
                "repos.manifest.toml",
                "redline.required_for_release",
                "optional_redline_requiredness_changed",
                &[AUDIT],
            );
        }
        if let Some(path) = text(redline, "contract_manifest")
            .filter(|path| relative(path) && path.ends_with("/Cargo.toml"))
        {
            let lock = format!("{}Cargo.lock", path.strip_suffix("Cargo.toml").unwrap());
            if let Some(manifest) = report.structured(root, path, "toml", &[REDLINE]) {
                report.cargo_manifest(path, &lock, &manifest, &Value::Null, &[REDLINE]);
            }
            if let Some(value) = report.structured(root, &lock, "toml", &[REDLINE]) {
                report.cargo_lock(&lock, &value, &[REDLINE]);
            }
        } else {
            report.gap(
                "repos.manifest.toml",
                "redline.contract_manifest",
                "optional_redline_contract_unresolved",
                &[REDLINE],
            );
        }
    }
    components.sort();
    components.dedup();
    for (name, base) in components {
        let prefix = if base.is_empty() {
            String::new()
        } else {
            format!("{base}/")
        };
        let caps = if base.is_empty() {
            &[AUDIT][..]
        } else {
            &[MIRRORS][..]
        };
        let policy = format!("{prefix}agent/audit-policy.toml");
        if let Some(value) = report.structured(root, &policy, "toml", caps) {
            let minimum = value["minimum_score"].as_u64();
            if let Some(minimum) = minimum.filter(|value| (85..=100).contains(value)) {
                let minimum = (minimum as u8).max(crate::audit_census::effective_floor(&name));
                report.floors.insert(format!("neverhuman/{name}"), minimum);
                report.observe(
                    &policy,
                    "minimum_score",
                    "repository-policy",
                    &name,
                    caps,
                    json!({"declared_minimum":value["minimum_score"],"effective_minimum":minimum}),
                );
            } else {
                report.error(
                    &policy,
                    "minimum_score",
                    "invalid_or_underfloor_repository_policy",
                    caps,
                );
            }
        }
        let workflows = format!("{prefix}.github/workflows");
        for path in directory_files(&mut report, root, &workflows, caps) {
            if (path.ends_with(".yml") || path.ends_with(".yaml"))
                && let Some(source) = report.load(root, &path, "workflow-literals", caps)
            {
                report.workflow(&path, &source, caps);
            }
        }
        let image = format!("{prefix}images/agent-sandbox/Dockerfile");
        if existing(root, &image)
            && let Some(source) = report.load(root, &image, "dockerfile-literals", &[IMAGE])
        {
            report.dockerfile(&image, &source);
        }
        for relative in ["agent/proof-lanes.toml", "agent/ci-lanes.toml"] {
            let path = format!("{prefix}{relative}");
            if existing(root, &path) {
                let _ = report.structured(root, &path, "toml", caps);
                report.gap(
                    &path,
                    "proof-declaration",
                    "proof_command_execution_graph_unresolved",
                    caps,
                );
            }
        }
        for relative in [
            "ops/ci/pr-ci.sh",
            "scripts/ci-local.sh",
            "Justfile",
            "ops/ci/security.sh",
            "ops/ci/ensure-jankurai.sh",
        ] {
            let path = format!("{prefix}{relative}");
            if existing(root, &path) {
                let _ = report.load(root, &path, "opaque-entrypoint", caps);
                report.gap(
                    &path,
                    "execution",
                    "entrypoint_execution_graph_unresolved",
                    caps,
                );
            }
        }
    }
    for path in [
        "components/jeryu-release-ops/repos.manifest.toml",
        "components/jeryu-deploy/repos.manifest.toml",
    ] {
        if let Some(value) = report.structured(root, path, "toml", &[MIRRORS]) {
            report.observe(path,"manifest","retained-release-projection",path,&[MIRRORS],
                json!({"schema_version":value.get("schema_version"),"manifest_authority":value.get("manifest_authority"),
                    "status":value.get("status"),"authority":"retained projection; root handover remains pending"}));
            report.gap(
                path,
                "manifest",
                "retained_release_projection_not_release_admission",
                &[MIRRORS],
            );
        }
    }
    for (path, caps) in [
        ("scripts/build.sh", REQUIRED),
        ("scripts/install.sh", &[APP][..]),
        ("scripts/ci.sh", &[AUDIT, MIRRORS][..]),
        ("scripts/audit.sh", &[AUDIT][..]),
        ("ops/ci/monorepo.sh", &[AUDIT, MIRRORS][..]),
        ("tools/security-lane.sh", &[AUDIT, MIRRORS][..]),
        ("ops/ci/public-dependency-sources.sh", &[AUDIT][..]),
        (
            "components/jeryu-deploy/ops/ci/security-tools.sh",
            &[MIRRORS][..],
        ),
        (
            "components/jeryu-deploy/ops/ci/cache-tools.sh",
            &[MIRRORS][..],
        ),
    ] {
        let _ = report.load(root, path, "opaque-entrypoint", caps);
        report.gap(
            path,
            "execution",
            "entrypoint_execution_graph_unresolved",
            caps,
        );
    }
    report.gap(
        "tools/security-lane.sh",
        "advisory-db",
        "advisory_database_revision_not_structured",
        &[AUDIT, MIRRORS],
    );
    report.gap(
        "components/jeryu-deploy/ops/ci/cache-tools.sh",
        "public-api",
        "public_api_executable_identity_not_structured",
        &[AUDIT, MIRRORS],
    );
    if let Some(inventory) = report.structured(
        root,
        "agent/audit-repositories.json",
        "json",
        &[AUDIT, MIRRORS, IMAGE, REDLINE],
    ) {
        report.compare_enrollment(&inventory);
    }
    report.finish()
}
