#!/usr/bin/env bash
# Exercise the real score gates with in-memory report and policy fixtures.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
python3 - <<'PY'
import io
import json
import unittest
from pathlib import Path
from unittest.mock import patch

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


class ScorePolicyTests(unittest.TestCase):
    def test_score_gates(self):
        scripts = ["ops/ci/score.sh", "components/jeryu-core/ops/ci/score.sh"]
        policies = [
            ("minimum_score = 85\n", {"score": 85}, True),
            ("minimum_score = 85\n", {"score": 84}, False),
            ("minimum_score = 90\n", {"score": 89}, False),
            ("minimum_score = 90\n", {"score": 90}, True),
            ("minimum_score = 100\n", {"score": 100}, True),
            ("minimum_score = 85\n", {"score": 100, "caps_applied": ["cap"]}, False),
            ("minimum_score = 85\n", {"score": 100, "caps": ["cap"]}, False),
            ("minimum_score = 85\n", {"score": 100, "hard_findings": 1}, False),
            ("minimum_score = 85\n", {"score": 100, "decision": {"hard_findings": 1}}, False),
        ]
        for invalid_report in [
            {"score": 100, "decision": {"hard_findings": 0}, "hard_findings": 1},
            {"score": True}, {"score": "100"}, {"score": 100.0}, {"score": 101},
            {"score": 100, "caps_applied": {}},
            {"score": 100, "caps_applied": [], "caps": ["concealed-cap"]},
            {"score": 100, "findings": {}},
            {"score": 100, "findings": [{"severity": "high"}]},
            {"score": 100, "findings": [{"severity": "critical"}]},
            {"score": 100, "findings": [{"severity": "low", "hardness": "hard"}]},
            {"score": 100, "findings": [{"severity": "unknown"}]},
            {"score": 100, "decision": {"hard_findings": -1}},
            {"score": 100, "decision": {"hard_findings": True}},
        ]:
            policies.append(("minimum_score = 85\n", invalid_report, False))
        for invalid in ["", "minimum_score = [", "minimum_score = true",
                        'minimum_score = "85"', "minimum_score = 85.0",
                        "minimum_score = 84", "minimum_score = 101"]:
            policies.append((invalid, {"score": 100}, False))
        for script in scripts:
            source = Path(script).read_text()
            body = source.split("python3 - <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
            code = compile(body, script, "exec")
            for policy, report, expected in policies:
                with self.subTest(script=script, policy=policy, report=report):
                    documents = {
                        "agent/audit-policy.toml": policy,
                        ".jankurai/repo-score.json": json.dumps({"caps_applied": [], "findings": [], "decision": {}, **report}),
                    }

                    def read_document(path):
                        return documents[str(path)]

                    with patch.object(Path, "read_text", read_document), patch("sys.stderr", new=io.StringIO()):
                        try:
                            exec(code, {"__name__": "__main__"})
                        except (KeyError, tomllib.TOMLDecodeError):
                            passed = False
                        except SystemExit as error:
                            passed = error.code == 0
                        else:
                            passed = True
                    self.assertEqual(passed, expected)
        print(f"{len(scripts) * len(policies)} score policy/report cases passed")


unittest.main()
PY
