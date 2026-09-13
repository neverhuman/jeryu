fn lifecycle(script: &str) {
    // Keep these small fixtures on success too: no directory-retirement proof
    // is implied by this synthetic create/closure regression.
    super::lifecycle(&format!("retain_fixture=1\n{script}"));
}

#[test]
fn builder_admits_before_start_and_verifies_successful_exit_and_removal() {
    lifecycle(
        r#"
rm "${control}/cid"
change_container '.Mounts += [{Type:"tmpfs",Destination:"/tmp",RW:true}]'
launch_fixture
test "$(cat "${test_root}/engine.calls")" = $'create\ninspect\nstart\ninspect\ninspect\nrm\nls'
for mode in create-no-id create-partial create-wrong-label start-fail nonzero-exit; do
  reset_container
  rm "${control}/cid"
  fixture_mode="${mode}"
  if launch_fixture >"${test_root}/launch.log" 2>&1; then exit 1; fi
  if [[ "${mode}" == create-* ]]; then
    ! grep -qx start "${test_root}/engine.calls"
  fi
  ! grep -qx rm "${test_root}/engine.calls"
done
# A failed create may have written its CID. Only that admitted identity is retired.
reset_container
rm "${control}/cid"
fixture_mode=create-partial
if launch_fixture >"${test_root}/launch.log" 2>&1; then exit 1; fi
fixture_mode=ok
container_cleanup
test "${container_removed}" = 1
test "$(cat "${test_root}/engine.calls")" = $'create\ninspect\nrm\nls'
"#,
    );
}

#[test]
fn create_request_and_response_bind_actual_dispatch_without_granting_custody() {
    lifecycle(
        r#"
rm "${control}/cid"
launch_fixture
jq -e --slurpfile actual "${control}/actual-argv.json" --arg docker "${docker_bin}" \
  --arg docker_sha "$(sha256sum "${docker_bin}" | cut -d' ' -f1)" \
  --arg timeout_sha "$(sha256sum /usr/bin/timeout | cut -d' ' -f1)" \
  --arg host "unix://${docker_socket}" --arg config "${scratch}/docker-config" '
  .schema == "jeryu.jankurai-container-create-request/v1"
  and .command.executable == $docker
  and .command.executable_sha256 == $docker_sha
  and .command.argv == (["--host",$host,"--config",$config] + $actual[0])
  and .timeout.executable == "/usr/bin/timeout"
  and .timeout.executable_sha256 == $timeout_sha
  and .timeout.seconds == 120 and .timeout.kill_after_seconds == 2
  and .timeout.signal == "TERM" and .timeout.foreground == true
  and .environment == {clear:true,PATH:"/usr/bin:/bin"}
' "${control}/create-request.json" >/dev/null
jq -e --arg request "$(sha256sum "${control}/create-request.json" | cut -d' ' -f1)" \
  --arg out "$(sha256sum "${control}/create.stdout" | cut -d' ' -f1)" \
  --arg err "$(sha256sum "${control}/create.stderr" | cut -d' ' -f1)" '
  .schema == "jeryu.jankurai-container-create-response/v1"
  and .actual_command_exit == 0 and .request_sha256 == $request
  and .stdout_sha256 == $out and .stderr_sha256 == $err
  and (.started_at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T"))
  and .ended_at >= .started_at and .container_ownership_verified == false
' "${control}/create-response.json" >/dev/null
test "$(stat -c '%a:%h' "${control}/create-request.json")" = 600:1
test "$(stat -c '%a:%h' "${control}/create-response.json")" = 600:1
# A receipt path already occupied is not overwritten, and no new create starts.
printf 'preserved request\n' >"${control}/create-request.json"
: >"${test_root}/engine.calls"
if launch_fixture >"${test_root}/launch.log" 2>&1; then exit 1; fi
test "$(cat "${control}/create-request.json")" = 'preserved request'
test ! -s "${test_root}/engine.calls"
"#,
    );
}

#[test]
fn create_failure_and_timeout_survive_unknown_closure_without_start_or_removal() {
    lifecycle(
        r#"
fixture_cleanup=1
for scenario in 'create-no-id:23' 'create-timeout:124' 'create-killed:137' 'create-success-no-id:0'; do
  reset_container
  rm "${control}/cid"
  fixture_mode="${scenario%:*}" expected="${scenario#*:}"
  if launch_fixture >"${control}/launch.log" 2>&1; then exit 1; else result=$?; fi
  test "${result}" = 1 # Cleanup uncertainty is separate from the actual command exit.
  jq -e --argjson expected "${expected}" '.actual_command_exit == $expected
    and .container_ownership_verified == false' "${control}/create-response.json" >/dev/null
  test "$(cat "${test_root}/engine.calls")" = create
  test ! -e "${control}/cid" && test -d "${scratch}" && test -d "${source_root}"
  grep -F 'container closure unknown' "${control}/launch.log" >/dev/null
  grep -F "create response: exit=${expected} retained" "${control}/launch.log" >/dev/null
  if [[ "${expected}" == 23 ]]; then
    test "$(cat "${control}/create.stderr")" = 'synthetic create refusal'
  elif [[ "${expected}" == 0 ]]; then
    test "$(cat "${control}/create.stdout")" = "${fixture_id}"
  else
    test ! -s "${control}/create.stdout" && test ! -s "${control}/create.stderr"
  fi
done
# An error after writing the real private CID permits existing custody cleanup,
# but keeps its failure response and never attaches to the created object.
reset_container
rm "${control}/cid"
fixture_mode=create-partial
if launch_fixture >"${control}/launch.log" 2>&1; then exit 1; else result=$?; fi
test "${result}" = 23
test "$(cat "${test_root}/engine.calls")" = $'create\ninspect\nrm\nls'
jq -e '.actual_command_exit == 23' "${control}/create-response.json" >/dev/null
grep -F 'build_exit=23 create_exit=23' "${control}/launch.log" >/dev/null
test -d "${scratch}" && test -d "${source_root}"
"#,
    );
}

#[test]
fn failure_after_create_preserves_stage_and_successful_create_response() {
    lifecycle(
        r#"
fixture_cleanup=1
rm "${control}/cid"
stage="${test_root}/output/retained-stage"
printf 'unchanged staged output\n' >"${stage}"
stage_identity="$(stat -c '%d:%i:%u' "${stage}")"
fixture_mode=start-fail
if launch_fixture >"${control}/launch.log" 2>&1; then exit 1; else result=$?; fi
test "${result}" = 24
test "$(cat "${test_root}/engine.calls")" = $'create\ninspect\nstart\ninspect\nrm\nls'
jq -e '.actual_command_exit == 0' "${control}/create-response.json" >/dev/null
test "$(cat "${stage}")" = 'unchanged staged output'
test "$(stat -c '%d:%i:%u' "${stage}")" = "${stage_identity}"
grep -F 'build_exit=24 create_exit=0' "${control}/launch.log" >/dev/null
test -d "${scratch}" && test -d "${source_root}"
"#,
    );
}
