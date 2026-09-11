use super::*;

pub(crate) fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("constant renderer regex")
}

pub(super) fn replace_pin_block(text: &str, pin: &Pin) -> Result<String, String> {
    let begin_pattern = regex(&format!(r"(?m)^{}$", regex::escape(PIN_MARKER_BEGIN)));
    let end_pattern = regex(&format!(r"(?m)^{}$", regex::escape(PIN_MARKER_END)));
    let begin_matches: Vec<_> = begin_pattern.find_iter(text).collect();
    let end_matches: Vec<_> = end_pattern.find_iter(text).collect();
    match (begin_matches.as_slice(), end_matches.as_slice()) {
        ([], []) => {
            let set_pattern = regex(r"(?m)^set -euo pipefail[ \t]*$");
            let set_matches: Vec<_> = set_pattern.find_iter(text).collect();
            if set_matches.is_empty() {
                return Ok(text.to_owned());
            }
            if set_matches.len() != 1 {
                return Err(format!(
                    "unmarked Jankurai pin consumer must contain exactly one \
`set -euo pipefail`; found {}",
                    set_matches.len()
                ));
            }
            let insertion = set_matches[0].end();
            Ok(format!(
                "{}\n\n{}{}",
                &text[..insertion],
                pin.shell_block(),
                &text[insertion..]
            ))
        }
        ([begin], [end]) if begin.start() < end.start() => {
            let mut rendered = String::with_capacity(text.len() + pin.shell_block().len());
            rendered.push_str(&text[..begin.start()]);
            rendered.push_str(&pin.shell_block());
            rendered.push_str(&text[end.end()..]);
            Ok(rendered)
        }
        _ => Err(format!(
            "Jankurai pin consumer has malformed generated markers: begin={} end={}",
            begin_matches.len(),
            end_matches.len()
        )),
    }
}

pub(super) fn replace_require_function(text: &str, function: &str) -> String {
    regex(r"(?ms)^require_jankurai\(\) \{.*?^\}")
        .replace(text, |_: &Captures<'_>| function.to_owned())
        .into_owned()
}

pub(super) fn bind_jankurai_wrapper(text: &str) -> Result<String, String> {
    let wrapper_start = regex(r"(?m)^jankurai\(\)[ \t]*\{[ \t]*$");
    let wrapper_starts: Vec<_> = wrapper_start.find_iter(text).collect();
    if wrapper_starts.is_empty() {
        let separator = if text.ends_with("\n\n") {
            ""
        } else if text.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        return Ok(format!("{text}{separator}{CANONICAL_JANKURAI_WRAPPER}\n"));
    }
    if wrapper_starts.len() != 1 {
        return Err(format!(
            "Jankurai command wrapper must be unique; found {}",
            wrapper_starts.len()
        ));
    }

    let canonical_wrapper = regex(&format!(
        r"(?m)^{}$",
        regex::escape(CANONICAL_JANKURAI_WRAPPER)
    ));
    if canonical_wrapper.find_iter(text).count() == 1 {
        return Ok(text.to_owned());
    }

    let exact_wrapper = regex(
        r#"(?m)^jankurai\(\)[ \t]*\{[ \t]*\n[ \t]+require_jankurai(?: \|\| return 1)?[ \t]*\n[ \t]+command "\$\{(?P<bin>JERYU_(?:GOVERNED_)?JANKURAI_BIN)\}" "\$@"[ \t]*\n\}[ \t]*$"#,
    );
    let captures = exact_wrapper.captures(text).ok_or_else(|| {
        "Jankurai command wrapper must contain only governed verification and exact-bin execution"
            .to_owned()
    })?;
    let wrapper_count = exact_wrapper.captures_iter(text).count();
    if wrapper_count != 1 {
        return Err(format!(
            "Jankurai command wrapper must have one exact implementation; found {wrapper_count}"
        ));
    }
    let bin = captures
        .name("bin")
        .ok_or_else(|| "Jankurai command wrapper is missing its executable binding".to_owned())?;
    match bin.as_str() {
        "JERYU_GOVERNED_JANKURAI_BIN" | "JERYU_JANKURAI_BIN" => {
            let mut rendered = text.to_owned();
            let matched = captures.get(0).expect("matched exact predecessor wrapper");
            rendered.replace_range(matched.range(), CANONICAL_JANKURAI_WRAPPER);
            Ok(rendered)
        }
        _ => Err("Jankurai command wrapper uses an unsupported executable binding".to_owned()),
    }
}

pub(super) fn replace_workflow_pin(text: &str, pin: &Pin) -> String {
    let Some(start) = text.find("env:\n") else {
        return text.to_owned();
    };
    if start > 0 && !text[..start].ends_with('\n') {
        return text.to_owned();
    }
    let body_start = start + "env:\n".len();
    let mut body_end = body_start;
    for line in text[body_start..].split_inclusive('\n') {
        if !line.starts_with("  ") {
            break;
        }
        body_end += line.len();
    }
    let known: BTreeSet<&str> = PIN_ENV_FIELDS.iter().map(|(name, _)| *name).collect();
    let mut retained: Vec<String> = Vec::new();
    let mut generated = false;
    for line in text[body_start..body_end].lines() {
        let stripped = line.trim();
        if stripped == WORKFLOW_PIN_MARKER_BEGIN {
            generated = true;
            continue;
        }
        if stripped == WORKFLOW_PIN_MARKER_END {
            generated = false;
            continue;
        }
        if generated {
            continue;
        }
        let key = stripped.split_once(':').map_or(stripped, |(key, _)| key);
        if !known.contains(key) {
            retained.push(line.to_owned());
        }
    }
    retained.push(pin.workflow_block());
    format!(
        "{}{}\n{}",
        &text[..body_start],
        retained.join("\n"),
        &text[body_end..]
    )
}

pub(crate) fn semantic_identity_rules(text: &str, pin: &Pin) -> String {
    let text = regex(
        r"(?:https://github\.com/neverhuman/jankurai\.git|http://127\.0\.0\.1:8787/git/jeryu/jankurai\.git)",
    )
    .replace_all(text, pin.get("repo"));
    let text = regex(r"\bv\d+\.\d+\.\d+-deadlang-precision(?:-split\.\d+)?\b")
        .replace_all(&text, pin.get("tag"));
    let text = regex(r"\bjankurai \d+\.\d+\.\d+\b").replace_all(&text, pin.get("version"));
    regex(r"\bJankurai \d+\.\d+\.\d+\b")
        .replace_all(&text, pin.get("version").replace("jankurai", "Jankurai"))
        .into_owned()
}

pub(super) fn replace_native_tool_block(text: &str, pin: &Pin) -> String {
    let marker = "[[tools]]";
    let mut output = String::new();
    let mut offset = 0;
    while let Some(relative) = text[offset..].find(marker) {
        let start = offset + relative;
        output.push_str(&text[offset..start]);
        let next = text[start + marker.len()..]
            .find(&format!("\n{marker}"))
            .map_or(text.len(), |value| start + marker.len() + value);
        let block = &text[start..next];
        if regex(r#"(?m)^id\s*=\s*"jankurai"\s*$"#).is_match(block) {
            let block = regex(r#"(version\s*=\s*")[^"]*(")"#)
                .replace_all(block, format!("${{1}}{}${{2}}", pin.get("semver")));
            let block = regex(r#"(observed_path\s*=\s*")[^"]*(")"#)
                .replace_all(&block, "${1}/home/ubuntu/.jeryu/bin/jankurai${2}");
            output.push_str(&regex(r#"(observed_digest\s*=\s*")[^"]*(")"#).replace_all(
                &block,
                format!("${{1}}sha256:{}${{2}}", pin.get("binary_sha256")),
            ));
        } else {
            output.push_str(block);
        }
        offset = next;
    }
    output.push_str(&text[offset..]);
    output
}

pub(super) fn remove_install_step(text: &str) -> String {
    let Some(start) = text.find("      - name: Install pinned jankurai\n") else {
        return text.to_owned();
    };
    let remaining = &text[start + 1..];
    let next = remaining
        .find("\n      - name:")
        .or_else(|| remaining.find("\n      #"))
        .map_or(text.len(), |value| start + 1 + value + 1);
    format!(
        "{}      - name: Verify governed jankurai\n        run: |\n          source \
ops/ci/lib.sh\n          require_jankurai\n{}",
        &text[..start],
        &text[next..]
    )
}

pub(super) fn replace_rust_string_constant(text: &str, name: &str, value: &str) -> String {
    let pattern = regex(&format!(
        r#"(?ms)(const {}: &str\s*=\s*)"[^"]*"(\s*;)"#,
        regex::escape(name)
    ));
    let encoded = serde_json::to_string(value).expect("validated identity string");
    pattern
        .replace_all(text, |captures: &Captures<'_>| {
            format!("{}{}{}", &captures[1], encoded, &captures[2])
        })
        .into_owned()
}

pub(super) fn replace_hex_on_marked_line(
    text: &str,
    marker: &str,
    width: usize,
    value: &str,
) -> String {
    let line = regex(&format!(r"(?m)^.*{}.*$", regex::escape(marker)));
    let digest = regex(&format!(r"[0-9a-f]{{{width}}}"));
    line.replace_all(text, |captures: &Captures<'_>| {
        digest.replace(&captures[0], value).into_owned()
    })
    .into_owned()
}

pub(super) fn replace_receipt_path(text: &str, receipt_sha256: &str) -> String {
    regex(r"/opt/jeryu/receipts/jankurai/sha256/[0-9a-f]{64}\.json")
        .replace_all(
            text,
            format!("{SANDBOX_RECEIPT_ROOT}/{receipt_sha256}.json"),
        )
        .into_owned()
}

pub(super) fn replace_ci_bridge_constants(
    text: &str,
    pin: &Pin,
    context: &RenderContext,
) -> String {
    let mut rendered = text.to_owned();
    for (name, key) in [
        ("GOVERNED_JANKURAI_VERSION", "version"),
        ("GOVERNED_JANKURAI_SHA256", "binary_sha256"),
        ("GOVERNED_JANKURAI_SOURCE_REPO", "repo"),
        ("GOVERNED_JANKURAI_SOURCE_TAG", "tag"),
        ("GOVERNED_JANKURAI_SOURCE_REV", "rev"),
        ("GOVERNED_JANKURAI_SOURCE_TREE", "source_tree"),
        (
            "GOVERNED_JANKURAI_SOURCE_ARCHIVE_SHA256",
            "source_archive_sha256",
        ),
        ("GOVERNED_JANKURAI_CARGO_LOCK_SHA256", "cargo_lock_sha256"),
        ("GOVERNED_JANKURAI_RUSTC_VERSION", "rustc_version"),
        ("GOVERNED_JANKURAI_CARGO_VERSION", "cargo_version"),
        ("GOVERNED_JANKURAI_TARGET_TRIPLE", "target_triple"),
        ("GOVERNED_JANKURAI_BUILD_MODE", "build_mode"),
    ] {
        rendered = replace_rust_string_constant(&rendered, name, pin.get(key));
    }
    for (name, value) in [
        (
            "GOVERNED_JANKURAI_MANIFEST_COMMIT",
            context.authority.commit.as_str(),
        ),
        (
            "GOVERNED_JANKURAI_MANIFEST_TREE",
            context.authority.tree.as_str(),
        ),
        (
            "GOVERNED_JANKURAI_MANIFEST_SHA256",
            context.authority.sha256.as_str(),
        ),
    ] {
        rendered = replace_rust_string_constant(&rendered, name, value);
    }
    rendered
}

pub(super) fn replace_governance_test_constants(
    text: &str,
    pin: &Pin,
    context: &RenderContext,
) -> String {
    let mut rendered = text.to_owned();
    for (name, value) in [
        ("TAG", pin.get("tag")),
        ("REV", pin.get("rev")),
        ("TREE", pin.get("source_tree")),
        ("ARCHIVE_SHA256", pin.get("source_archive_sha256")),
        ("BINARY_SHA256", pin.get("binary_sha256")),
        ("MANIFEST_COMMIT", context.authority.commit.as_str()),
        ("MANIFEST_TREE", context.authority.tree.as_str()),
        ("MANIFEST_SHA256", context.authority.sha256.as_str()),
        (
            "IMAGE_RECEIPT_SHA256",
            context.image_receipt_sha256.as_str(),
        ),
    ] {
        rendered = replace_rust_string_constant(&rendered, name, value);
    }
    rendered
}
