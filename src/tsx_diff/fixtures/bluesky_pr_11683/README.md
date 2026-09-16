# Bluesky social-app PR #11683: client.ts

Source: https://github.com/bluesky-social/social-app/pull/11683/files
Upstream file: `src/analytics/metrics/client.ts`

- Original revision: `8e52eba582581756eab73c538db5b53deeda4f77`
- Modified revision: `7bb3a0a14b947c77676824ba5faf99ff93d6c976`
- Original Git blob prefix: `1ea0573dcd1`
- Modified Git blob: `2f3b8a45cf5960815ce79ba26825dae932a7a884`

The snapshots preserve the full original file contents and line numbers.
`change.diff` contains only the original Git patch for `client.ts`.
The PR's `client.test.ts` changes are deliberately excluded.
Tests embed these fixtures and run without network access.

Expected declaration changes:

- Added variables: `MAX_BACKOFF_MS`, `MIN_BACKOFF_MS`.
- Added properties: `MetricsClient::backoffMs`, `MetricsClient::backoffUntil`.
- Added method: `MetricsClient::trim`.
- Changed methods: `MetricsClient::flush`, `MetricsClient::sendBatch`.
- Changed enclosing class: `MetricsClient`.

Unchanged declarations, including `start`, `track`, `retryFailedLogs`, and
local variables whose line numbers shifted, are not reported.
This is plain TypeScript accepted by the TSX grammar. There are no JSX inputs;
the current analyzer does not yet report function calls as child changes.

The upstream MIT license is reproduced in LICENSE and applies to these fixtures.
