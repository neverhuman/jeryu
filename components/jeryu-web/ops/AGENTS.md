# Ops Agent Guidance

This directory owns local CI wrappers, release-support scripts, and audit evidence lanes.
Keep hosted workflows thin: they should delegate to scripts under `ops/ci/` so every gate can be reproduced locally.
