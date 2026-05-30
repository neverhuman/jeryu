# Webhook and app token migration

JeryuMirror migrates webhook and app *metadata*, not live secret values.
This is required for safe backup/restore because exporting a backup must not turn
into a credential exfiltration channel.

## Webhooks

Imported webhook records include:

- delivery URL
- active flag
- event list
- optional content type
- target secret name
- migration notes

When the source export indicates a secret/token existed, JeryuMirror records a
secret name such as `jeryu-mirror/webhook/7`. Operators must create that secret in
the destination secret store before enabling the webhook.

## GitHub Apps

Imported app installation records include:

- installation id
- app slug
- permission keys
- subscribed events
- target token secret name
- migration notes

Installation tokens are not exportable. After restore, reinstall or rotate the
app and bind the new credential to the named secret.

## Agent rule

Agents may request a restore plan and may create missing secret placeholders, but
must not receive or print secret values.
