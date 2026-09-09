# Runner contract instructions

This directory owns navigation and consumer guidance for the endpoint-neutral
runner wire contract. The checked structural mirror lives in `../schemas/`;
the authoritative serde types and validators live in
`../crates/jeryu-runner-protocol/src/wire/`.

Do not add credentials, transport configuration, service lifecycle, durable
state, or deployment authority to the message contract. Do not describe JSON
Schema validation as equivalent to Rust `ValidateWire`, and do not change a
wire field, enum, discriminator, bound, or schema shape in isolation.

Every contract change must run `just contract-drift`. Before publication, run the
full protected-repository gate in `ops/ci/pr-ci.sh`.
