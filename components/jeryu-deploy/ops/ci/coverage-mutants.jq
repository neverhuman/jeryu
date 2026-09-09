# cargo-mutants v25.3.1: src/{outcome,scenario,mutant,lab}.rs.
# A complete normal test run contains one successful Build/Test baseline and
# exactly one terminal outcome for every selected mutant. The auditor still
# enforces the separate changed-path survivor threshold after this check.
def natural: type == "number" and . >= 0 and floor == .;
def nonempty: type == "string" and length > 0;
def failure: type == "object" and keys == ["Failure"] and (.Failure | natural and . > 0);
def phases($names):
  (.phase_results | type == "array") and
  ([.phase_results[].phase] == $names) and
  all(.phase_results[]; (.duration | type == "number" and . >= 0) and
      (.argv | type == "array" and length > 0 and all(.[]; type == "string")));
def completed:
  if .summary == "Success" and .scenario == "Baseline" then
    phases(["Build", "Test"]) and all(.phase_results[]; .process_status == "Success")
  elif .summary == "CaughtMutant" then
    phases(["Build", "Test"]) and .phase_results[0].process_status == "Success" and
    (.phase_results[1].process_status | failure)
  elif .summary == "MissedMutant" then
    phases(["Build", "Test"]) and all(.phase_results[]; .process_status == "Success")
  elif .summary == "Unviable" then
    phases(["Build"]) and (.phase_results[0].process_status | failure)
  else false end;
length == 1 and (.[0] |
($selected | length == 1) and ($selected[0] | type == "array" and length > 0) and
($locks | length == 1) and ($locks[0].cargo_mutants_version == "25.3.1") and
($selected[0] | all(.[]; type == "object" and .package == $package and
  (.file | nonempty) and (.replacement | type == "string") and
  (.span.start.line | natural and . > 0) and (.span.end.line | natural and . > 0))) and
($selected[0] | length == (unique | length)) and
type == "object" and
([.total_mutants, .missed, .caught, .timeout, .unviable, .success] | all(.[]; natural)) and
.timeout == 0 and .success == 0 and
.total_mutants == ($selected[0] | length) and
.total_mutants == (.caught + .missed + .unviable) and
(.caught + .missed > 0) and
(.outcomes | type == "array" and length == ($selected[0] | length) + 1) and
([.outcomes[] | select(.scenario == "Baseline")] | length == 1) and
all(.outcomes[]; completed) and
([.outcomes[] | select(.scenario != "Baseline") | .scenario] |
  all(.[]; type == "object" and keys == ["Mutant"])) and
([.outcomes[] | select(.scenario != "Baseline") | .scenario.Mutant] | sort) == ($selected[0] | sort) and
([.outcomes[] | select(.summary == "CaughtMutant")] | length) == .caught and
([.outcomes[] | select(.summary == "MissedMutant")] | length) == .missed and
([.outcomes[] | select(.summary == "Unviable")] | length) == .unviable and
(if $rc == 0 then .missed == 0 elif $rc == 2 then .missed > 0 else false end))
