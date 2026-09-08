# AgentBridge Typed API

This archive implements AgentBridge as an in-process Rust API. The method names map directly to the Phase 7 HTTP surface:

| HTTP target | Rust method |
| --- | --- |
| `GET /api/agent/context?repo=&pr=` | `AgentBridge::context` |
| `GET /api/agent/mergeability?pr=` | `AgentBridge::mergeability` |
| `POST /api/agent/dry-run/patch` | `AgentBridge::dry_run_patch` |
| `POST /api/agent/proof-plan` | `AgentBridge::proof_plan` |
| `POST /api/agent/run-proof` | `AgentBridge::run_proof` |
| `POST /api/agent/propose-fix` | `AgentBridge::propose_fix` |
| `POST /api/agent/hotfix` | `AgentBridge::hotfix` |
| `GET /api/agent/receipts/{id}` | `AgentBridge::receipt` |

The type layer enforces SHA binding, path scopes, receipts, and proof witnesses.
