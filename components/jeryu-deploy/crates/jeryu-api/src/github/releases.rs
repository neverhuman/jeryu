//! Hosted release resources are not implemented. Git tags are separate Git
//! objects and do not establish a release resource or uploaded assets.

use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{Pagination, error_response, json_response, paginate};

impl GithubRouter {
    pub(super) fn list_releases(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        match self.core.get_repository(owner, repo) {
            // Releases are not stored in the forge domain yet, so the list is
            // always empty; still paginate so the route honors ?per_page/?page
            // and stays shape-consistent with the other list routes.
            Ok(_) => paginate(path, page, &Vec::<Value>::new(), |slice, _total| {
                Value::Array(slice.to_vec())
            }),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_release(&self, owner: &str, repo: &str) -> Response {
        match self.core.get_repository(owner, repo) {
            Ok(_) => json_response(
                501,
                &json!({
                    "message": "Hosted release creation is not implemented",
                    "documentation_url": "https://github.com/neverhuman/jeryu/blob/main/docs/release.md",
                    "jeryu_repair_hint": "Git tags can be pushed through Git; hosted release resources and assets are unavailable. No release or tag was created.",
                }),
            ),
            Err(err) => error_response(err),
        }
    }
}
