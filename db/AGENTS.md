# db/ — monorepo root

The product root does not own a SQLite schema. Durable tables live in the
owning component (`components/jeryu-intelligence/db`, Work, cache). Do not add
migrations, constraints, or ad-hoc SQL at this root. Score and CI treat this
directory as documentation of that boundary.
