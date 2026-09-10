use super::*;

impl Inventory {
    pub(super) fn workflow(&mut self, path: &str, source: &str, caps: &[&str]) {
        self.gap(
            path,
            "workflow",
            "workflow_execution_graph_unresolved",
            caps,
        );
        let mut block_indent = None;
        for (index, line) in source.lines().enumerate() {
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if block_indent.is_some_and(|start| indent > start) {
                continue;
            }
            block_indent = None;
            let line = line
                .trim_start()
                .strip_prefix("- ")
                .unwrap_or(line.trim_start());
            if let Some((_, value)) = line.split_once(':')
                && value.trim_start().starts_with(['|', '>'])
            {
                block_indent = Some(indent);
            }
            let Some(value) = line.strip_prefix("uses:") else {
                continue;
            };
            let location = format!("line:{}", index + 1);
            let Some(value) = literal(value) else {
                self.gap(path, &location, "dynamic_or_unsupported_workflow_use", caps);
                continue;
            };
            if value.starts_with("./") {
                self.observe(
                    path,
                    &location,
                    "workflow-local-action",
                    path,
                    caps,
                    json!({"local_path":value}),
                );
                self.gap(path, &location, "local_action_execution_not_expanded", caps);
                continue;
            }
            if let Some(image) = value.strip_prefix("docker://") {
                self.image(path, &location, image, path, caps);
                continue;
            }
            let Some((repository, reference)) = value.rsplit_once('@') else {
                self.gap(path, &location, "workflow_reference_missing", caps);
                continue;
            };
            let url = format!("https://github.com/{repository}");
            let slug = github_slug(&url);
            if slug.is_none() {
                self.gap(path, &location, "workflow_repository_unresolved", caps);
            }
            if !oid(reference) {
                self.gap(path, &location, "workflow_reference_not_immutable", caps);
            }
            self.observe(path,&location,"workflow-action",path,caps,
                json!({"repository":slug,"commit":oid(reference).then_some(reference),
                    "reference":reference,"action_path":repository.split('/').skip(2).collect::<Vec<_>>().join("/"),"license":null}));
        }
    }
    pub(super) fn image(
        &mut self,
        path: &str,
        location: &str,
        image: &str,
        consumer: &str,
        caps: &[&str],
    ) {
        // Docker references have no URL userinfo, query, fragment or whitespace.
        if !image_locator(image) {
            self.gap(
                path,
                location,
                "unsupported_or_sensitive_image_locator",
                caps,
            );
            self.observe(path,location,"container-image",consumer,caps,
                json!({"image":null,"locator_sha256":hash(image.as_bytes()),"sha256":null,"repository":null,"commit":null,"license":null}));
            return;
        }
        let pin = image.rsplit_once("@sha256:").map(|(_, digest)| digest);
        if !pin.is_some_and(digest) {
            self.gap(path, location, "image_digest_missing_or_invalid", caps);
        }
        self.observe(
            path,
            location,
            "container-image",
            consumer,
            caps,
            json!({"image":image,"sha256":pin,"repository":null,"commit":null,"license":null}),
        );
    }
    pub(super) fn dockerfile(&mut self, path: &str, source: &str) {
        self.gap(
            path,
            "recipe",
            "image_build_execution_graph_unresolved",
            &[IMAGE],
        );
        let mut stages = BTreeSet::new();
        let mut instruction = String::new();
        let mut first = 0usize;
        for (index, line) in source.lines().enumerate() {
            let line = line.trim();
            if instruction.is_empty() && (line.is_empty() || line.starts_with('#')) {
                continue;
            }
            if instruction.is_empty() {
                first = index + 1;
            }
            instruction.push_str(line.strip_suffix('\\').unwrap_or(line));
            if line.ends_with('\\') {
                instruction.push(' ');
                continue;
            }
            let location = format!("line:{first}");
            let (verb, args) = instruction
                .split_once(char::is_whitespace)
                .unwrap_or((&instruction, ""));
            match verb.to_ascii_uppercase().as_str() {
                "FROM" => {
                    let args: Vec<_> = args.split_whitespace().collect();
                    let image = args.iter().find(|arg| !arg.starts_with("--"));
                    if let Some(image) = image {
                        if image.contains('$') {
                            self.gap(path, &location, "dynamic_image_base", &[IMAGE]);
                        } else if *image != "scratch"
                            && !stages.contains(&image.to_ascii_lowercase())
                        {
                            self.image(path, &location, image, path, &[IMAGE]);
                        }
                    } else {
                        self.error(path, &location, "invalid_image_base", &[IMAGE]);
                    }
                    if let Some(index) = args.iter().position(|arg| arg.eq_ignore_ascii_case("AS"))
                        && let Some(stage) = args.get(index + 1)
                    {
                        stages.insert(stage.to_ascii_lowercase());
                    }
                }
                "ARG" => {
                    if let Some((key, value)) = args.split_once('=')
                        && key.starts_with("JANKURAI_")
                        && (key.ends_with("REPO")
                            || key.ends_with("REV")
                            || key.ends_with("TAG")
                            || key.ends_with("SHA256")
                            || key.ends_with("IMAGE"))
                    {
                        if let Some(value) = literal(value) {
                            let safe = if key.ends_with("REPO") {
                                safe_url(value)
                            } else if key.ends_with("IMAGE") {
                                self.image(path, &location, value, key, &[IMAGE]);
                                image_locator(value).then(|| value.to_owned())
                            } else {
                                Some(value.to_owned())
                            };
                            if safe.is_none() {
                                self.gap(
                                    path,
                                    &location,
                                    "unsupported_or_sensitive_image_locator",
                                    &[IMAGE],
                                );
                            }
                            self.observe(
                                path,
                                &location,
                                "image-auditor-argument",
                                path,
                                &[IMAGE],
                                json!({"argument":key,"value":safe,"overridable":true}),
                            );
                        } else {
                            self.gap(path, &location, "dynamic_image_auditor_argument", &[IMAGE]);
                        }
                    }
                }
                "RUN" => {
                    self.gap(path, &location, "opaque_image_run_instruction", &[IMAGE]);
                    if args
                        .split_whitespace()
                        .any(|word| word == "@jeryu/jekko-cli")
                    {
                        self.observe(path,&location,"unresolved-image-package",path,&[IMAGE],
                            json!({"package":"@jeryu/jekko-cli","repository":null,"commit":null,"version":null,"license":null}));
                    }
                }
                _ => {}
            }
            instruction.clear();
        }
        if !instruction.is_empty() {
            self.error(path, "eof", "unfinished_image_instruction", &[IMAGE]);
        }
    }
}

fn image_locator(image: &str) -> bool {
    !image.is_empty()
        && !image.contains("://")
        && !image.contains(['?', '#'])
        && image
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-:@".contains(&byte))
        && image.matches('@').count() <= 1
        && (!image.contains('@') || image.contains("@sha256:"))
}

fn literal(value: &str) -> Option<&str> {
    let value = value.trim();
    let value = if value.starts_with(['\'', '"']) {
        let quote = value.as_bytes()[0] as char;
        value.strip_prefix(quote)?.strip_suffix(quote)?
    } else {
        value.split(" #").next()?.trim_end()
    };
    (!value.is_empty()
        && !value.contains(['$', '\\', '{', '}', '\n', '\r'])
        && !value.starts_with(['*', '&', '[']))
    .then_some(value)
}
