# Candidate installation evidence is deliberately a separate closed schema.
# $renderer and $pin come from the current exact, clean source checkout.
def fields($names): type == "object" and keys == ($names | sort);
def digest: type == "string" and test("^[0-9a-f]{64}$");

fields(["schema", "timestamp", "operator", "run_id", "test_mode", "source",
  "build", "governance", "binary", "installation", "conclusion",
  "renderer_metadata", "renderer_metadata_sha256", "inputs", "verification"])
and .schema == "jeryu.jankurai-public-candidate-installation/v1"
and .test_mode == false and .conclusion == "success"
and (.timestamp | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
and (.operator | type == "string" and length > 0)
and (.run_id | type == "string" and length > 0)
and .renderer_metadata == $renderer
and .renderer_metadata_sha256 == $renderer_sha
and .verification == {build:"verified", installation:"verified", public_readback:"verified"}
and (.inputs | fields(["pin", "builder"]))
and .inputs.pin == {path:"components/jeryu-tool/generated/jankurai-pin.env", blob:$pin_blob, sha256:$pin_sha}
and .inputs.builder == {path:"components/jeryu-tool/ops/build-jankurai-hermetic.sh", blob:$builder_blob, sha256:$builder_sha}
and (.source | fields(["remote", "producer_repository", "tag", "commit", "tree",
  "archive_sha256", "cargo_lock_sha256", "verification"]))
and .source.remote == $renderer.distribution.repository
and .source.producer_repository == $pin.JANKURAI_REPO
and .source.tag == $pin.JANKURAI_TAG and .source.commit == $pin.JANKURAI_REV
and .source.tree == $pin.JANKURAI_SOURCE_TREE
and .source.archive_sha256 == $pin.JANKURAI_SOURCE_ARCHIVE_SHA256
and .source.cargo_lock_sha256 == $pin.JANKURAI_CARGO_LOCK_SHA256
and .source.verification == "public-candidate"
and (.governance | fields(["status", "manifest_repo", "manifest_commit", "manifest_tree",
  "manifest_sha256", "protected_main", "protection_policy", "handover", "predecessor_authentication"]))
and .governance.status == "public-candidate" and .governance.protected_main == false
and .governance.handover == "pending" and .governance.predecessor_authentication == "not-performed"
and .governance.protection_policy == "not-applicable"
and .governance.manifest_repo == $renderer.source.repository
and .governance.manifest_commit == $renderer.source.commit
and .governance.manifest_tree == $renderer.source.tree
and .governance.manifest_sha256 == $renderer.manifest.sha256
and (.binary | fields(["sha256", "version_output"]))
and .binary.sha256 == $pin.JANKURAI_BINARY_SHA256 and .binary.version_output == $pin.JANKURAI_VERSION
and (.installation | fields(["path", "atomic", "previous_binary_sha256", "rollback_artifact", "lock"]))
and .installation.path == $binary_path and .installation.atomic == true
and ((.installation.previous_binary_sha256 == "" and .installation.rollback_artifact == "")
  or ((.installation.previous_binary_sha256 | digest)
    and .installation.rollback_artifact == ($install_root + "/rollback/jankurai/" + .installation.previous_binary_sha256)))
and .installation.lock == {path:($install_root + "/.jankurai-install.lock"), identity:$lock_identity,
  exclusive:true, held_through_receipt:true}
and (.build | fields(["rustc", "cargo", "target_triple", "mode", "package_path",
  "builder_image", "builder_image_id", "linker", "glibc", "vendor_files_sha256",
  "vendor_file_count", "cargo_config_sha256", "environment", "rustflags", "command",
  "context_sha256", "cargo_net_offline", "closed_vendor", "network_none", "read_only_root",
  "non_root", "capabilities_dropped", "no_new_privileges", "container_engine_path",
  "git_global_config_disabled", "git_system_config_disabled", "git_http_follow_redirects",
  "git_terminal_prompt", "jankurai_update_check", "network_scope", "no_proxy"]))
and .build.rustc == $pin.JANKURAI_RUSTC_VERSION and .build.cargo == $pin.JANKURAI_CARGO_VERSION
and .build.target_triple == $pin.JANKURAI_TARGET_TRIPLE and .build.mode == $pin.JANKURAI_BUILD_MODE
and .build.package_path == $pin.JANKURAI_PACKAGE_PATH
and .build.builder_image == $pin.JANKURAI_BUILDER_IMAGE and .build.builder_image_id == $pin.JANKURAI_BUILDER_IMAGE_ID
and .build.linker == $pin.JANKURAI_LINKER_VERSION and .build.glibc == $pin.JANKURAI_GLIBC_VERSION
and .build.vendor_files_sha256 == $pin.JANKURAI_VENDOR_FILES_SHA256
and .build.vendor_file_count == $pin.JANKURAI_VENDOR_FILE_COUNT
and .build.cargo_config_sha256 == $pin.JANKURAI_CARGO_CONFIG_SHA256
and .build.environment == $pin.JANKURAI_BUILD_ENVIRONMENT and .build.rustflags == $pin.JANKURAI_RUSTFLAGS
and .build.command == $pin.JANKURAI_BUILD_COMMAND and .build.context_sha256 == $pin.JANKURAI_BUILD_CONTEXT_SHA256
and .build.cargo_net_offline == true and .build.closed_vendor == true and .build.network_none == true
and .build.read_only_root == true and .build.non_root == true and .build.capabilities_dropped == true
and .build.no_new_privileges == true and .build.container_engine_path == "/usr/bin/docker"
and .build.git_global_config_disabled == true and .build.git_system_config_disabled == true
and .build.git_http_follow_redirects == false and .build.git_terminal_prompt == false
and .build.jankurai_update_check == false
and .build.network_scope == "public-source-fetch-plus-closed-vendor-network-none"
and .build.no_proxy == ""
