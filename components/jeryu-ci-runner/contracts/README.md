# Runner wire contract

`schemas/jeryu.runner.v1.schema.json` is the checked structural mirror of the
endpoint-neutral `jeryu.runner.v1` JSON protocol implemented in
`crates/jeryu-runner-protocol/src/wire/`.

The Rust serde types and their `ValidateWire` implementations remain the
runtime authority. Consumers must deserialize and validate with that crate;
the JSON Schema is not a substitute for the Rust checks that bind contexts,
digests, receipts, timestamps, uniqueness, path safety, and reserved
authentication environment names.

The schema deliberately exposes no credential or transport fields. Transport
authentication is supplied outside the message body. Optional Rust fields are
required JSON properties whose value may be `null`, matching serde's emitted
shape.

Run `just contract-drift` after changing any wire type, discriminator, enum, or
schema. The drift test compares all 16 public object fields, all five wire
enums, all eight top-level message variants, fixed discriminators, and public
collection bounds directly with the Rust sources.
