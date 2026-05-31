//! Front-end that compiles GitHub Actions and native TOML CI definitions into
//! the deterministic [`Pipeline`] IR.
//!
//! This module is a thin re-export hub over cohesive submodules grouped by
//! responsibility:
//! - [`error`]: entry-point configuration and error types.
//! - `lexer`: indentation-aware line lexing and shared scalar helpers.
//! - `github` / `native`: per-format parsers.
//! - `steps` / `matrix` / `attributes`: shared job-attribute lowering.
//! - `pipeline`: assembly of parsed jobs into a validated pipeline.

mod attributes;
mod error;
mod github;
mod lexer;
mod matrix;
mod native;
mod pipeline;
mod steps;

pub use error::{CiKind, CompileContext, CompileError};

use jeryu_ci_ir::{Pipeline, deterministic_hash};

use crate::github::compile_github;
use crate::native::compile_native;

#[derive(Default)]
pub struct Compiler;

impl Compiler {
    pub fn compile(
        input: &str,
        kind: CiKind,
        context: CompileContext,
    ) -> Result<Pipeline, CompileError> {
        if input.trim().is_empty() {
            return Err(CompileError::EmptyInput);
        }
        let mut pipeline = match kind {
            CiKind::GitHubActions => compile_github(input, &context)?,
            CiKind::NativeToml => compile_native(input, &context)?,
        };
        pipeline.id = deterministic_hash(&format!(
            "pipeline|{}|{}|{}|{}",
            pipeline.source.as_str(),
            pipeline.repo,
            pipeline.commit,
            pipeline.ir_hash()
        ));
        pipeline
            .validate()
            .map_err(|err| CompileError::Validation(err.to_string()))?;
        Ok(pipeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> CompileContext {
        CompileContext::new("acme/demo", "abc123")
    }

    #[test]
    fn compiles_github_matrix_and_hash_is_deterministic() -> Result<(), Box<dyn std::error::Error>>
    {
        let input = include_str!("../../../tests/fixtures/github/matrix.yml");
        let a = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        let b = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        assert_eq!(a.ir_hash(), b.ir_hash());
        assert_eq!(a.jobs.len(), 5);
        assert!(
            a.jobs
                .iter()
                .any(|job| job.id == "test__os_ubuntu-latest__toolchain_stable")
        );
        assert!(
            a.edges
                .iter()
                .any(|edge| edge.from == "fmt" && edge.to.contains("test__"))
        );
        Ok(())
    }

    #[test]
    fn compiles_native_toml() -> Result<(), Box<dyn std::error::Error>> {
        let input = include_str!("../../../tests/fixtures/native/ci.toml");
        let pipeline = Compiler::compile(input, CiKind::NativeToml, ctx())?;
        assert_eq!(pipeline.jobs.len(), 3);
        assert!(
            pipeline
                .edges
                .iter()
                .any(|edge| edge.from == "check" && edge.to == "test")
        );
        Ok(())
    }

    #[test]
    fn parses_block_form_needs() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
name: block needs
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: cargo check
  test:
    runs-on: ubuntu-latest
    needs:
      - build
    steps:
      - run: cargo test
"#;
        let pipeline = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        assert!(
            pipeline
                .edges
                .iter()
                .any(|edge| edge.from == "build" && edge.to == "test")
        );
        Ok(())
    }

    #[test]
    fn parses_multiline_run_body() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
name: multiline
jobs:
  script:
    runs-on: ubuntu-latest
    steps:
      - name: shell body
        run: |
          echo prepare
          cargo test --workspace
"#;
        let pipeline = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        let command = pipeline.jobs[0].steps[0]
            .command
            .as_deref()
            .expect("run command");
        assert!(command.contains("echo prepare"));
        assert!(command.contains("cargo test --workspace"));
        assert_ne!(command, "|");
        Ok(())
    }

    #[test]
    fn github_job_without_executable_step_fails_closed() {
        let input = r#"
name: missing step
jobs:
  inspect:
    runs-on: ubuntu-latest
    steps:
      - name: metadata only
"#;
        let err = Compiler::compile(input, CiKind::GitHubActions, ctx())
            .expect_err("metadata-only step must fail closed");
        assert_eq!(err, CompileError::MissingSteps("inspect".to_string()));
    }

    #[test]
    fn parses_500_job_fixture() -> Result<(), Box<dyn std::error::Error>> {
        let input = include_str!("../../../tests/fixtures/github/500_jobs.yml");
        let pipeline = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        assert_eq!(pipeline.jobs.len(), 500);
        assert_eq!(pipeline.edges.len(), 499);
        let hash = pipeline.ir_hash();
        assert!(hash.starts_with("fnv64:"));
        Ok(())
    }
}
