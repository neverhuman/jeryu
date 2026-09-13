#[test]
fn builder_binds_engine_handle_to_immutable_repository_and_platform() {
    super::run(
        r#"
sed -n '/^builder_image_id() {$/,/^}$/p' "${builder_file}" >"${test_root}/image-admission.sh"
test -s "${test_root}/image-admission.sh"
source "${test_root}/image-admission.sh"
engine_id="sha256:$(printf 'b%.0s' {1..64})"
for handle in "${engine_id}" "${JANKURAI_BUILDER_IMAGE_ID}"; do
  jq -n --arg id "${handle}" --arg repo "${JANKURAI_BUILDER_IMAGE}" \
    '[{Id:$id,RepoDigests:[$repo],Os:"linux",Architecture:"amd64"}]' >"${test_root}/image.json"
  test "$(builder_image_id "${test_root}/image.json")" = "${handle}"
done
for mutation in '.[0].RepoDigests=[]' '.[0].RepoDigests=["rust:latest"]' \
  'del(.[0].RepoDigests)' '.[0].RepoDigests="invalid"' \
  '.[0].Architecture="arm64"' '.[0].Os="windows"' \
  '.[0].Id="sha256:short"' '.[0].Id=null' '.[0].Id="sha256:$(whoami)"' \
  '. + .' '.=[]' '.[0]'; do
  jq "${mutation}" "${test_root}/image.json" >"${test_root}/invalid.json"
  if builder_image_id "${test_root}/invalid.json" >"${test_root}/result" 2>/dev/null; then exit 1; fi
  test ! -s "${test_root}/result"
done
for value in '' '{' '{} {}'; do
  printf '%s' "${value}" >"${test_root}/invalid.json"
  if builder_image_id "${test_root}/invalid.json" >"${test_root}/result" 2>/dev/null; then exit 1; fi
  test ! -s "${test_root}/result"
done
JANKURAI_BUILDER_IMAGE_ID="${engine_id}"
if builder_image_id "${test_root}/image.json" >/dev/null 2>&1; then exit 1; fi
"#,
    );
}

#[test]
fn created_container_must_use_the_admitted_engine_handle() {
    super::lifecycle(
        r#"
actual_image_id="sha256:$(printf 'b%.0s' {1..64})"
reset_container
rm "${control}/cid"
launch_fixture
test "$(cat "${test_root}/engine.calls")" = $'create\ninspect\nstart\ninspect\ninspect\nrm\nls'
# Even the pinned index value is refused as a container handle when the engine
# admitted a different handle. No start or removal may follow this mismatch.
reset_container
change_container ".Image=\"${JANKURAI_BUILDER_IMAGE_ID}\""
rm "${control}/cid"
if launch_fixture >"${test_root}/failure.log" 2>&1; then exit 1; fi
test "$(cat "${test_root}/engine.calls")" = $'create\ninspect'
if container_cleanup; then exit 1; fi
! grep -Eq '^(start|rm)$' "${test_root}/engine.calls"
"#,
    );
}
