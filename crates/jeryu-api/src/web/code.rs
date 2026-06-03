//! gitd-backed repository source reads for tree/blob/raw/search routes.
//!
//! These helpers read only from the Jeryu-managed bare mirrors. They do not
//! inspect arbitrary local paths, and they reject path/ref shapes that could be
//! interpreted as command options or filesystem traversal.

use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::Repository;
use jeryu_gitd::RepoId;
use jeryu_readmodel::contracts::{
    BlobEncoding, BlobResponse, RenderedMarkdown, RepositoryId, TreeEntry, TreeEntryKind,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

use super::WebState;
use super::markdown::render_markdown;
use super::repositories::repo_id;

const DEFAULT_SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_LIMIT: usize = 200;

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct CodeReadQuery {
    #[serde(rename = "ref", alias = "refName")]
    pub ref_name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct CodeSearchQuery {
    #[serde(rename = "ref", alias = "refName")]
    pub ref_name: Option<String>,
    pub path: Option<String>,
    #[serde(alias = "query")]
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CodeSearchResult {
    pub repo: RepositoryId,
    pub ref_name: String,
    pub path: String,
    pub line: u32,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub(super) struct CodeReadError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl CodeReadError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_input",
            message: message.into(),
        }
    }

    fn storage(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "storage_failed",
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CodeReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl IntoResponse for CodeReadError {
    fn into_response(self) -> AxumResponse {
        (
            self.status,
            Json(json!({
                "code": self.code,
                "message": self.message,
                "jeryu_repair_hint": {
                    "purpose": "read repository source from the Jeryu-managed gitd mirror",
                    "reason": self.code,
                    "common_fixes": [
                        "verify the repository id, ref, and path",
                        "refresh the local repository import before retrying"
                    ],
                    "docs_url": "docs/errors.md#not-found",
                    "repair_hint": "rerun cargo test -p jeryu-api --features web --jobs 40"
                }
            })),
        )
            .into_response()
    }
}

pub(super) fn tree_response(
    state: &WebState,
    repo: &Repository,
    query: CodeReadQuery,
) -> Result<Vec<TreeEntry>, CodeReadError> {
    let mirror = open_mirror(state, repo)?;
    let ref_name = normalized_ref(query.ref_name.as_deref(), &repo.default_branch)?;
    let commit = resolve_commit(state, &mirror.path, &ref_name)?;
    let path = normalized_path(query.path.as_deref())?;
    let treeish = if path.is_empty() {
        commit.clone()
    } else {
        ensure_object_type(state, &mirror.path, &commit, &path, "tree")?;
        format!("{commit}:{path}")
    };
    let output = run_git_bytes(state, &mirror.path, &["ls-tree", "-z", "-l", &treeish])?;
    parse_ls_tree(&output, &path)
}

pub(super) fn blob_response(
    state: &WebState,
    repo: &Repository,
    query: CodeReadQuery,
) -> Result<BlobResponse, CodeReadError> {
    let BlobRead {
        path,
        ref_name,
        sha,
        size,
        bytes,
    } = read_blob(state, repo, query)?;
    let mime = mime_for_path(&path).to_string();
    let rendered_markdown = if mime == "text/markdown" {
        std::str::from_utf8(&bytes).ok().map(render_markdown)
    } else {
        None
    };
    let (encoding, text, base64, is_binary) = blob_body(bytes);
    Ok(BlobResponse {
        repo: repo_id(repo),
        path,
        ref_name,
        sha: sha.trim().to_string(),
        size_bytes: size,
        mime,
        encoding,
        text,
        base64,
        rendered_markdown,
        is_binary,
    })
}

pub(super) fn raw_response(
    state: &WebState,
    repo: &Repository,
    query: CodeReadQuery,
) -> Result<AxumResponse, CodeReadError> {
    let BlobRead { path, bytes, .. } = read_blob(state, repo, query)?;
    Ok(([(header::CONTENT_TYPE, mime_for_path(&path))], bytes).into_response())
}

pub(super) fn search_response(
    state: &WebState,
    repo: &Repository,
    query: CodeSearchQuery,
) -> Result<Vec<CodeSearchResult>, CodeReadError> {
    let q = query.q.trim();
    if q.is_empty() {
        return Err(CodeReadError::invalid_input("search query is required"));
    }
    let mirror = open_mirror(state, repo)?;
    let ref_name = normalized_ref(query.ref_name.as_deref(), &repo.default_branch)?;
    let commit = resolve_commit(state, &mirror.path, &ref_name)?;
    let path = normalized_path(query.path.as_deref())?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    let mut args = vec!["grep", "-n", "-I", "-F", "--full-name", "-e", q, &commit];
    let path_arg;
    if !path.is_empty() {
        args.push("--");
        path_arg = path.clone();
        args.push(&path_arg);
    }
    let output = run_git_bytes_allow_status(state, &mirror.path, &args, &[0, 1])?;
    if output.is_empty() {
        return Ok(Vec::new());
    }
    let text = String::from_utf8_lossy(&output);
    let mut results = Vec::new();
    for line in text.lines() {
        if let Some(result) = parse_grep_line(repo, &ref_name, line) {
            results.push(result);
            if results.len() >= limit {
                break;
            }
        }
    }
    Ok(results)
}

pub(super) fn head_for_repo(state: &WebState, repo: &Repository) -> Option<String> {
    let mirror = open_mirror(state, repo).ok()?;
    let ref_name = normalized_ref(None, &repo.default_branch).ok()?;
    let commit = resolve_commit(state, &mirror.path, &ref_name).ok()?;
    Some(format!(
        "{ref_name}@{}",
        commit.chars().take(12).collect::<String>()
    ))
}

fn open_mirror(
    state: &WebState,
    repo: &Repository,
) -> Result<jeryu_gitd::repo::Repository, CodeReadError> {
    let id = RepoId::new(&repo.owner, &repo.name)
        .map_err(|err| CodeReadError::invalid_input(format!("invalid repository id: {err}")))?;
    state.repo_manager.open(&id).map_err(|err| {
        CodeReadError::not_found(format!(
            "managed gitd mirror not found for {}: {err}",
            repo.full_name
        ))
    })
}

struct BlobRead {
    path: String,
    ref_name: String,
    sha: String,
    size: u64,
    bytes: Vec<u8>,
}

fn read_blob(
    state: &WebState,
    repo: &Repository,
    query: CodeReadQuery,
) -> Result<BlobRead, CodeReadError> {
    let mirror = open_mirror(state, repo)?;
    let ref_name = normalized_ref(query.ref_name.as_deref(), &repo.default_branch)?;
    let commit = resolve_commit(state, &mirror.path, &ref_name)?;
    let path = normalized_path(query.path.as_deref())?;
    if path.is_empty() {
        return Err(CodeReadError::invalid_input("blob path is required"));
    }
    ensure_object_type(state, &mirror.path, &commit, &path, "blob")?;
    let object = format!("{commit}:{path}");
    let sha = run_git_text(state, &mirror.path, &["rev-parse", &object])?
        .trim()
        .to_string();
    let size = run_git_text(state, &mirror.path, &["cat-file", "-s", &object])?
        .trim()
        .parse::<u64>()
        .map_err(|err| CodeReadError::storage(format!("invalid blob size: {err}")))?;
    let bytes = run_git_bytes(state, &mirror.path, &["cat-file", "-p", &object])?;
    Ok(BlobRead {
        path,
        ref_name,
        sha,
        size,
        bytes,
    })
}

fn normalized_ref(input: Option<&str>, default_branch: &str) -> Result<String, CodeReadError> {
    let value = input.unwrap_or(default_branch).trim();
    if value.is_empty() {
        return Err(CodeReadError::invalid_input("ref must not be empty"));
    }
    if value.starts_with('-') || value.contains('\0') || value.contains("..") {
        return Err(CodeReadError::invalid_input("ref has an unsafe shape"));
    }
    Ok(value.to_string())
}

fn normalized_path(input: Option<&str>) -> Result<String, CodeReadError> {
    let value = input.unwrap_or("").trim().trim_start_matches('/');
    if value.is_empty() {
        return Ok(String::new());
    }
    if value.contains('\0') || value.starts_with('-') || value.contains('\\') {
        return Err(CodeReadError::invalid_input("path has an unsafe shape"));
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        if part.is_empty() || part == "." || part == ".." || part == ".git" {
            return Err(CodeReadError::invalid_input("path has an unsafe shape"));
        }
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn resolve_commit(
    state: &WebState,
    git_dir: &std::path::Path,
    ref_name: &str,
) -> Result<String, CodeReadError> {
    let rev = format!("{ref_name}^{{commit}}");
    let commit = run_git_text(state, git_dir, &["rev-parse", "--verify", &rev])?;
    Ok(commit.trim().to_string())
}

fn ensure_object_type(
    state: &WebState,
    git_dir: &std::path::Path,
    commit: &str,
    path: &str,
    expected: &str,
) -> Result<(), CodeReadError> {
    let object = format!("{commit}:{path}");
    let actual = run_git_text(state, git_dir, &["cat-file", "-t", &object])
        .map_err(|_| CodeReadError::not_found(format!("path not found at ref: {path}")))?;
    if actual.trim() == expected {
        Ok(())
    } else {
        Err(CodeReadError::invalid_input(format!(
            "path {path} is {}, expected {expected}",
            actual.trim()
        )))
    }
}

fn run_git_text(
    state: &WebState,
    git_dir: &std::path::Path,
    args: &[&str],
) -> Result<String, CodeReadError> {
    let bytes = run_git_bytes(state, git_dir, args)?;
    String::from_utf8(bytes)
        .map_err(|err| CodeReadError::storage(format!("git output was not UTF-8: {err}")))
}

fn run_git_bytes(
    state: &WebState,
    git_dir: &std::path::Path,
    args: &[&str],
) -> Result<Vec<u8>, CodeReadError> {
    run_git_bytes_allow_status(state, git_dir, args, &[0])
}

fn run_git_bytes_allow_status(
    state: &WebState,
    git_dir: &std::path::Path,
    args: &[&str],
    allowed: &[i32],
) -> Result<Vec<u8>, CodeReadError> {
    let git_dir = git_dir.to_string_lossy().to_string();
    let mut full_args = vec!["--git-dir", git_dir.as_str()];
    full_args.extend_from_slice(args);
    let output = Command::new(&state.repo_manager.config().git_bin)
        .args(&full_args)
        .output()
        .map_err(|err| CodeReadError::storage(format!("git command failed: {err}")))?;
    let code = output.status.code().unwrap_or(-1);
    if allowed.contains(&code) {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(CodeReadError::not_found(format!(
            "git command returned {code}: {stderr}"
        )))
    }
}

fn parse_ls_tree(output: &[u8], parent: &str) -> Result<Vec<TreeEntry>, CodeReadError> {
    let mut entries = Vec::new();
    for raw in output.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let item = String::from_utf8(raw.to_vec())
            .map_err(|err| CodeReadError::storage(format!("tree entry was not UTF-8: {err}")))?;
        let Some((meta, name)) = item.split_once('\t') else {
            return Err(CodeReadError::storage("tree entry missing path separator"));
        };
        let fields = meta.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 3 {
            return Err(CodeReadError::storage("tree entry missing metadata"));
        }
        let mode = fields[0];
        let object_type = fields[1];
        let sha = fields[2];
        let size = fields.get(3).and_then(|value| value.parse::<u64>().ok());
        let kind = match (object_type, mode) {
            ("tree", _) => TreeEntryKind::Directory,
            ("commit", _) => TreeEntryKind::Submodule,
            ("blob", "120000") => TreeEntryKind::Symlink,
            ("blob", _) => TreeEntryKind::File,
            _ => TreeEntryKind::File,
        };
        let path = if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        };
        entries.push(TreeEntry {
            path,
            name: name.to_string(),
            kind,
            sha: sha.to_string(),
            size_bytes: size,
            last_commit_sha: None,
            last_commit_message: None,
            last_commit_at: None,
        });
    }
    Ok(entries)
}

fn parse_grep_line(repo: &Repository, ref_name: &str, line: &str) -> Option<CodeSearchResult> {
    let (first, rest) = line.split_once(':')?;
    let (path, rest) = if looks_like_commit(first) {
        rest.split_once(':')?
    } else {
        (first, rest)
    };
    let (line_no, preview) = rest.split_once(':')?;
    let line_no = line_no.parse::<u32>().ok()?;
    Some(CodeSearchResult {
        repo: repo_id(repo),
        ref_name: ref_name.to_string(),
        path: path.to_string(),
        line: line_no,
        preview: preview.trim().to_string(),
    })
}

fn looks_like_commit(value: &str) -> bool {
    value.len() >= 7 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn mime_for_path(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "ts" | "tsx" | "jsx" | "rs" | "toml" | "yaml" | "yml" | "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

fn blob_body(bytes: Vec<u8>) -> (BlobEncoding, Option<String>, Option<String>, bool) {
    if bytes.contains(&0) {
        return (
            BlobEncoding::Base64,
            None,
            Some(encode_base64(&bytes)),
            true,
        );
    }
    match String::from_utf8(bytes) {
        Ok(text) => (BlobEncoding::Utf8, Some(text), None, false),
        Err(err) => {
            let bytes = err.into_bytes();
            (
                BlobEncoding::Base64,
                None,
                Some(encode_base64(&bytes)),
                true,
            )
        }
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[allow(dead_code)]
fn _assert_rendered_markdown_send_sync(_: Option<RenderedMarkdown>) {}
