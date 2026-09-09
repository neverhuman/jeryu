//! Bounded Git source browsing and README mutation handlers.

use super::*;

fn source_ref<'a>(repo: &'a Repository, query: &'a SourceQuery) -> &'a str {
    query
        .ref_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&repo.default_branch)
}

fn normalize_git_path(path: Option<&str>) -> SourceResult<String> {
    let raw = path.unwrap_or("");
    if raw.starts_with('/') || raw.contains('\0') {
        return Err(Box::new(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "path may not be absolute or contain NUL bytes",
        )));
    }
    let path = raw.trim_matches('/');
    if path.split('/').any(|part| matches!(part, "." | "..")) {
        return Err(Box::new(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "path may not contain . or .. segments",
        )));
    }
    Ok(path.to_string())
}

fn git_tree(
    state: &WebState,
    repo: &Repository,
    ref_name: &str,
    path: Option<&str>,
) -> SourceResult<Vec<TreeEntry>> {
    let path = normalize_git_path(path)?;
    let bare = state
        .repo_manager
        .open_parts(&repo.owner, &repo.name)
        .map_err(|_| {
            Box::new(api_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "repository storage not found",
            ))
        })?;
    let commit = resolve_commit(state, &bare, ref_name)?;
    let spec = if path.is_empty() {
        commit
    } else {
        format!("{commit}:{path}")
    };
    let out = git_output(state, &bare.path, &["ls-tree", "-z", "-l", &spec])?;
    let mut entries = Vec::new();
    for record in out
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let text = String::from_utf8_lossy(record);
        let Some((meta, name)) = text.split_once('\t') else {
            continue;
        };
        let parts: Vec<_> = meta.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let mode = parts[0];
        let object_kind = parts[1];
        let sha = parts[2].to_string();
        let size_bytes = parts[3].parse::<u64>().ok();
        let kind = match (mode, object_kind) {
            ("160000", _) => TreeEntryKind::Submodule,
            ("120000", _) => TreeEntryKind::Symlink,
            (_, "tree") => TreeEntryKind::Directory,
            _ => TreeEntryKind::File,
        };
        let full_path = if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}/{name}")
        };
        entries.push(TreeEntry {
            path: full_path,
            name: name.to_string(),
            kind,
            sha,
            size_bytes,
            last_commit_sha: None,
            last_commit_message: None,
            last_commit_at: None,
        });
    }
    entries.sort_by(|a, b| {
        let ak = if a.kind == TreeEntryKind::Directory {
            0
        } else {
            1
        };
        let bk = if b.kind == TreeEntryKind::Directory {
            0
        } else {
            1
        };
        ak.cmp(&bk).then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

fn git_blob(
    state: &WebState,
    repo: &Repository,
    ref_name: &str,
    path: &str,
    render: Option<&str>,
) -> SourceResult<BlobResponse> {
    let (bytes, mime, sha) = git_blob_bytes_inner(state, repo, ref_name, path)?;
    let text = if bytes.contains(&0) {
        None
    } else {
        String::from_utf8(bytes.clone()).ok()
    };
    let is_binary = text.is_none();
    let rendered_markdown = text
        .as_deref()
        .filter(|_| render == Some("html") && is_markdown_path(path))
        .map(render_markdown);
    Ok(BlobResponse {
        repo: repo_id(repo),
        path: normalize_git_path(Some(path))?,
        ref_name: ref_name.to_string(),
        sha,
        size_bytes: bytes.len() as u64,
        mime: mime.to_string(),
        encoding: if is_binary {
            BlobEncoding::Base64
        } else {
            BlobEncoding::Utf8
        },
        text,
        base64: if is_binary {
            Some(STANDARD.encode(&bytes))
        } else {
            None
        },
        rendered_markdown,
        is_binary,
    })
}

fn git_blob_bytes(
    state: &WebState,
    repo: &Repository,
    ref_name: &str,
    path: &str,
) -> SourceResult<(Vec<u8>, &'static str)> {
    let (bytes, mime, _) = git_blob_bytes_inner(state, repo, ref_name, path)?;
    Ok((bytes, mime))
}

fn git_blob_bytes_inner(
    state: &WebState,
    repo: &Repository,
    ref_name: &str,
    path: &str,
) -> SourceResult<(Vec<u8>, &'static str, String)> {
    let path = normalize_git_path(Some(path))?;
    let bare = state
        .repo_manager
        .open_parts(&repo.owner, &repo.name)
        .map_err(|_| {
            Box::new(api_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "repository storage not found",
            ))
        })?;
    let commit = resolve_commit(state, &bare, ref_name)?;
    let spec = format!("{commit}:{path}");
    let sha = String::from_utf8_lossy(&git_output(
        state,
        &bare.path,
        &["rev-parse", "--verify", &spec],
    )?)
    .trim()
    .to_string();
    if sha.is_empty() {
        return Err(Box::new(api_error(
            StatusCode::NOT_FOUND,
            "not_found",
            "file not found",
        )));
    }
    let size = git_object_size(state, &bare.path, &sha)?;
    let limit = preview_limit_for_path(&path);
    if size > limit {
        return Err(Box::new(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "blob_too_large",
            "file is too large to preview",
        )));
    }
    let bytes = git_output(state, &bare.path, &["cat-file", "-p", &sha])?;
    Ok((bytes, mime_for_path(&path), sha))
}

fn git_readme_path(
    state: &WebState,
    repo: &Repository,
    ref_name: &str,
) -> SourceResult<Option<String>> {
    let entries = git_tree(state, repo, ref_name, None)?;
    let mut candidates: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.kind == TreeEntryKind::File)
        .filter(|entry| entry.name.to_ascii_lowercase().starts_with("readme"))
        .map(|entry| entry.path)
        .collect();
    candidates.sort_by_key(|path| {
        if path.eq_ignore_ascii_case("README.md") {
            0
        } else {
            1
        }
    });
    Ok(candidates.into_iter().next())
}

fn resolve_commit(
    state: &WebState,
    bare: &jeryu_gitd::repo::Repository,
    ref_name: &str,
) -> SourceResult<String> {
    RefService::new((*state.repo_manager).clone())
        .resolve_commit(bare, ref_name)
        .map_err(|err| Box::new(git_source_response("resolve ref", &err.to_string())))?
        .ok_or_else(|| {
            Box::new(api_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "ref not found",
            ))
        })
}

fn git_output(state: &WebState, cwd: &std::path::Path, args: &[&str]) -> SourceResult<Vec<u8>> {
    let out = Command::new(&state.repo_manager.config().git_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|err| Box::new(git_source_response("run git", &err.to_string())))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(Box::new(git_source_response("run git", stderr.trim())))
    }
}

fn git_object_size(state: &WebState, cwd: &std::path::Path, sha: &str) -> SourceResult<u64> {
    let out = git_output(state, cwd, &["cat-file", "-s", sha])?;
    let text = String::from_utf8_lossy(&out);
    text.trim()
        .parse::<u64>()
        .map_err(|err| Box::new(git_source_response("read object size", &err.to_string())))
}

fn git_source_error(context: &str, err: impl std::fmt::Display) -> AxumResponse {
    git_source_response(context, &err.to_string())
}

fn git_source_response(context: &str, detail: &str) -> AxumResponse {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "git_source_failed",
        &format!("{context}: {detail}"),
    )
}

fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

fn preview_limit_for_path(path: &str) -> u64 {
    if is_markdown_path(path)
        && path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("readme"))
    {
        MAX_README_PREVIEW_BYTES
    } else {
        MAX_BLOB_PREVIEW_BYTES
    }
}

fn mime_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if is_markdown_path(&lower) {
        "text/markdown; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else {
        "text/plain; charset=utf-8"
    }
}

pub(in crate::web) async fn repo_refs(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    let bare = match state.repo_manager.open_parts(&repo.owner, &repo.name) {
        Ok(bare) => bare,
        Err(_) => {
            return Json(vec![RefSelectorItem {
                name: repo.default_branch.clone(),
                sha: "unknown".to_string(),
                kind: RefKind::Branch,
                protected: state
                    .github
                    .core()
                    .get_branch_protection(&repo.owner, &repo.name, &repo.default_branch)
                    .is_ok(),
            }])
            .into_response();
        }
    };
    let refs = match RefService::new((*state.repo_manager).clone()).list_refs(&bare) {
        Ok(refs) => refs,
        Err(err) => return git_source_error("list refs", err),
    };
    let mut items = Vec::new();
    for item in refs {
        if let Some(name) = item.name.strip_prefix("refs/heads/") {
            items.push(RefSelectorItem {
                name: name.to_string(),
                sha: item.oid,
                kind: RefKind::Branch,
                protected: state
                    .github
                    .core()
                    .get_branch_protection(&repo.owner, &repo.name, name)
                    .is_ok(),
            });
        } else if let Some(name) = item.name.strip_prefix("refs/tags/") {
            items.push(RefSelectorItem {
                name: name.to_string(),
                sha: item.oid,
                kind: RefKind::Tag,
                protected: false,
            });
        }
    }
    if items.is_empty() {
        items.push(RefSelectorItem {
            name: repo.default_branch.clone(),
            sha: "unknown".to_string(),
            kind: RefKind::Branch,
            protected: state
                .github
                .core()
                .get_branch_protection(&repo.owner, &repo.name, &repo.default_branch)
                .is_ok(),
        });
    }
    Json(items).into_response()
}

pub(in crate::web) async fn repo_tree(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<SourceQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    match git_tree(
        &state,
        &repo,
        source_ref(&repo, &query),
        query.path.as_deref(),
    ) {
        Ok(entries) => Json(entries).into_response(),
        Err(response) => *response,
    }
}

pub(in crate::web) async fn repo_blob(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<SourceQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    let Some(path) = query.path.as_deref().filter(|path| !path.trim().is_empty()) else {
        let readme = readme_markdown(&state, &repo);
        return Json(BlobResponse {
            repo: repo_id(&repo),
            path: "README.md".to_string(),
            ref_name: repo.default_branch,
            sha: "unknown".to_string(),
            size_bytes: readme.len() as u64,
            mime: "text/markdown".to_string(),
            encoding: BlobEncoding::Utf8,
            text: Some(readme.clone()),
            base64: None,
            rendered_markdown: Some(render_markdown(&readme)),
            is_binary: false,
        })
        .into_response();
    };
    match git_blob(
        &state,
        &repo,
        source_ref(&repo, &query),
        path,
        query.render.as_deref(),
    ) {
        Ok(blob) => Json(blob).into_response(),
        Err(response) => *response,
    }
}

pub(in crate::web) async fn repo_raw(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<SourceQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    let Some(path) = query.path.as_deref().filter(|path| !path.trim().is_empty()) else {
        return (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            readme_markdown(&state, &repo),
        )
            .into_response();
    };
    match git_blob_bytes(&state, &repo, source_ref(&repo, &query), path) {
        Ok((bytes, mime)) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        Err(response) => *response,
    }
}

pub(in crate::web) async fn repo_readme(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<SourceQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return readme_not_found_error();
    };
    if let Ok(Some(path)) = git_readme_path(&state, &repo, source_ref(&repo, &query)) {
        return match git_blob(
            &state,
            &repo,
            source_ref(&repo, &query),
            &path,
            Some("html"),
        ) {
            Ok(blob) => Json(ReadmeResponse {
                markdown: blob.text.unwrap_or_default(),
                rendered_markdown: blob
                    .rendered_markdown
                    .unwrap_or_else(|| render_markdown("")),
            })
            .into_response(),
            Err(response) => *response,
        };
    }
    Json(readme_response(&state, &repo)).into_response()
}

pub(in crate::web) async fn repo_readme_update(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return readme_not_found_error();
    };
    let request: ReadmeUpdateRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return api_error_with_hint(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_input",
                "readme update body failed validation",
                ApiErrorHint {
                    purpose: "update repository README",
                    reason: "invalid_input",
                    common_fixes: &[
                        "send JSON with a markdown string field",
                        "regenerate the managed README block from the fresh Jankurai artifact",
                    ],
                    docs_url: "docs/release-process.md#required-local-gates",
                    repair_hint: &format!(
                        "rerun bash ops/ci/publish-readme-score.sh --verify (body parse error: {error})"
                    ),
                },
            );
        }
    };
    match state
        .github
        .core()
        .set_repository_readme(&repo.owner, &repo.name, request.markdown)
    {
        Ok(markdown) => {
            Json(readme_response_with_markdown(&state, &repo, markdown)).into_response()
        }
        Err(ForgeError::NotFound(_)) => readme_not_found_error(),
        Err(ForgeError::Forbidden(err)) => api_error_with_hint(
            axum::http::StatusCode::FORBIDDEN,
            "forbidden",
            "repository README update was forbidden",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "forbidden",
                common_fixes: &[
                    "authenticate as an actor permitted to update this repository",
                    "preserve repository ownership and policy requirements",
                ],
                docs_url: "docs/errors.md#policy-denied",
                repair_hint: &format!("verify the authenticated actor before retrying ({err})"),
            },
        ),
        Err(ForgeError::WriterUnavailable(err)) => api_error_with_hint(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "writer_unavailable",
            "repository README writer is unavailable",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "writer_unavailable",
                common_fixes: &[
                    "inspect the current writer and backing resource custody",
                    "retry after writer ownership and storage identity are restored",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!("verify writer custody before retrying ({err})"),
            },
        ),
        Err(ForgeError::Storage(err)) => api_error_with_hint(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            "repository README could not be persisted",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "storage_failed",
                common_fixes: &[
                    "check the SQLite database path and write permissions",
                    "reopen the local forge store and rerun the publish helper",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (storage error: {err})"
                ),
            },
        ),
        Err(ForgeError::Conflict(err)) => api_error_with_hint(
            axum::http::StatusCode::CONFLICT,
            "conflict",
            "repository README update conflicted",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "conflict",
                common_fixes: &[
                    "refresh the local repo state before retrying the publish helper",
                    "replay the update against the latest README content",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (conflict: {err})"
                ),
            },
        ),
        Err(ForgeError::Validation(err)) => api_error_with_hint(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "repository README update failed validation",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "invalid_input",
                common_fixes: &[
                    "send a JSON body with a markdown string field",
                    "regenerate the managed score block from target/jankurai/repo-score.json",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (validation error: {err})"
                ),
            },
        ),
        Err(ForgeError::BranchProtection(err)) => api_error_with_hint(
            axum::http::StatusCode::FORBIDDEN,
            "policy_denied",
            "repository README update was blocked by policy",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "policy_denied",
                common_fixes: &[
                    "inspect the repository policy reason instead of bypassing the guard",
                    "supply the required proof or trust evidence",
                ],
                docs_url: "docs/errors.md#policy-denied",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (policy error: {err})"
                ),
            },
        ),
    }
}
