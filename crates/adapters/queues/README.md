# Queue adapters

Declared in `agent/boundaries.toml` (`queues.adapter_paths`).

Observable Rust queue adapters (Jansu clients today; Tansu / Iggy /
Fluvio under evaluation per the streaming exception in
`agent/boundaries.toml`) belong here. Domain code must depend on
typed event contracts in `contracts/events/`, never on a raw client.

This repository does not yet ship a queue adapter; the directory
exists so the boundary lane can route events through a real path
when one is added.
