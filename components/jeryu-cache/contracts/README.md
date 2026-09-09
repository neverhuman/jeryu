# Public contracts

The public wire contract owned here is the serialized
`jeryu_cache_core::CacheReceipt`. Its versioned schema is
[`schemas/jeryu-cache-core-receipt.schema.json`](../schemas/jeryu-cache-core-receipt.schema.json).

The receipt object is closed: unknown top-level fields are rejected, required
fields match the decoder, and optional digests may be omitted or use explicit
JSON `null` for compatibility. Emitted digests are 64 lowercase hexadecimal
characters. `just contract-schema` binds the field set, required fields, and
enum spellings to real Serde behavior. Public Rust API drift remains separately
bound to the immutable split.1 baseline by `just contract-drift`.
