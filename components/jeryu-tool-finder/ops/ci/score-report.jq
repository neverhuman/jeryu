# One complete standard audit with the owning component's maintained floor.
# Count the actual findings; an advisory decision is insufficient evidence.
def integer: type == "number" and . == floor;
def report_passes:
  type == "object"
  and ($minimum | integer and . >= 85 and . <= 100)
  and (.score | integer and . >= $minimum and . <= 100)
  and .raw_score == .score
  and .caps_applied == []
  and (if has("caps") then .caps == [] else true end)
  and (.findings | type == "array" and all(.[];
    type == "object"
    and (.severity == "medium" or .severity == "low" or .severity == "info")
    and .hardness == "soft"))
  and (if has("hard_findings") then .hard_findings == 0 else true end)
  and (.decision | type == "object")
  and .decision.hard_findings == 0
  and .decision.passed == true
  and .decision.status == "pass"
  and .decision.minimum_score == $minimum
  and .scope == {mode:"full",paths:[]}
  and .policy.mode == "standard"
  and .policy.minimum_score == $minimum
  and (.policy.fail_on | type == "array" and sort == ["critical","high"])
  and .copy_code.summary.hard_classes == 0
  and .copy_code.summary.hard_instances == 0
  and (.copy_code.status == "pass" or .copy_code.status == "review")
  and (.copy_code.classes | type == "array" and all(.[];
    type == "object" and .hard_fail == false and .effective_severity == "warning"));
length == 1 and (.[0] | report_passes)
