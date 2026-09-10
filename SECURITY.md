# Security reporting

Do not put vulnerability details, credentials, personal data or runtime exports
in public issues, pull requests or workflow logs.

Use the repository's private vulnerability reporting form if GitHub presents
one on the [Security page](https://github.com/neverhuman/jeryu/security).
Private reporting was disabled at the 2026-09-10 readback. Until a private
channel is available, open a [minimal issue](https://github.com/neverhuman/jeryu/issues/new)
requesting a private security contact, with no vulnerability details. Wait for
the maintainer to establish that channel before sending the report.

In the private report, include the affected commit or release, deployment
conditions, impact and a minimal reproduction with sensitive values removed.
Do not attach live tokens or a production data directory.

This source candidate has not completed release qualification. A maintained
security-support window and response-time commitment have not been published.
[Current status](docs/migration/STATUS.md) records the remaining gates; do not
infer release qualification from a source branch or a passing diagnostic job.
