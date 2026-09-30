# Slack OAuth and event relay

Single-instance Node.js 22.13+ service behind an HTTPS reverse proxy. It does not deploy itself. Desktop builds set `SLACK_RELAY_URL` to this service's public origin.

Required environment variables:

- `PUBLIC_URL`: HTTPS public origin (no path prefix).
- `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET`, `SLACK_SIGNING_SECRET`: Slack app credentials, server only.
- `TOKEN_ENCRYPTION_KEY`: persistent 32-byte random key encoded as base64; use a secret manager. Rotation requires decrypting/re-encrypting existing records before replacing it.
- `DATABASE_PATH`: persistent SQLite path (default `./data/slack.sqlite`). Restrict directory access to the service user.
- `PORT` (8787) and `HOST` (127.0.0.1). Bind privately behind the reverse proxy; enforce request/time/connection limits there.

Run `node services/slack-relay/server.mjs`. Run tests with `node --test services/slack-relay/*.test.mjs`.

## Slack app setup

1. Create the app, enable OAuth distribution as appropriate for your workspace policy.
2. Register `https://YOUR_DOMAIN/slack/callback` as redirect URI.
3. User scopes: `channels:history`, `channels:read`, `im:history`, `im:read`, `users:read`, `usergroups:read`, `dnd:read`.
4. Subscribe on behalf of users to `message.channels` and `message.im` at `https://YOUR_DOMAIN/slack/events`. This is a user installation, not a bot-only installation. Private channels and group DMs are outside this version's scope.
5. Token rotation is currently unsupported: leave it disabled for this app. The relay rejects rotating-token responses rather than silently expiring them.
6. Workspace app approval may be required. Verify installation, own DM, direct/broadcast/group mentions, exclusions and DND with real accounts before release.

OAuth callback state is single-use. A desktop-generated SHA-256 challenge protects the temporary device claim. The device session is stored in the OS credential store; the Slack user token stays encrypted in SQLite. Device sessions expire after 90 days. Disconnect removes the device record, encrypted token and pending queue; it does not revoke the workspace-wide Slack installation.

Each notification is routed only after a per-user conversation access check; public channels also require membership. DND and user-group membership are cached briefly. `@here` checks Slack presence. Slack channel mute rules are not replicated.

Message queues are bounded and memory-only (50/device, 15-minute retention), with explicit acknowledgement from the desktop. Restarting the relay drops queued messages and pending login flows. Slack event delivery is acknowledged before processing, so later network/API failures can drop a notification. This version is suitable for a controlled pilot; reliable production delivery requires a durable job queue/retry policy and operational monitoring. Never add message/token data to request logs.

The server must run as one instance: pending OAuth flows, replay suppression and queues are process-local. Do not horizontally scale it without shared state. Protect SQLite and its WAL/SHM files and back up the encryption key separately. The service never writes messages back to Slack.
