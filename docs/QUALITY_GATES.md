# Quality gates

Dokkomplekt deliberately separates source verification from release proof.

## Source gate

`npm run quality` runs TypeScript compilation, frontend tests, production web build,
all Python contracts and `static_quality_gate.py --source-only`. It is valid on a
review machine without Rust and explicitly reports `cargo_not_executed=true`.

## Release gate

`npm run quality:release` additionally requires a real Cargo toolchain and runs
metadata, rustfmt, workspace check, clippy with warnings denied and all Rust tests
with `--locked`. Packaging still requires the signed Cargo/RustSec/Windows evidence
checked by `scripts/assert_release_ready.py`.

A green source gate is never evidence of a releasable installer.

## License-server lock and trust-boundary tests

The Rust test suite contains concurrent activation tests under one in-memory write
lock and, when `DATABASE_URL` is available, across the real PostgreSQL connection
pool. These tests must prove that contention cannot exceed the machine-slot limit.
HTTP integration tests also require the per-order bearer token, verify first-machine
binding, enforce monotonic payment states and exercise request-rate limits.

Production YooKassa configuration is pinned to `https://api.yookassa.ru`; only
loopback mock origins are accepted outside production.


## Legacy-order recovery

Orders created before per-order access tokens were introduced retain a null token hash and remain fail-closed on public status and activation routes. Recovery is an explicit administrative operation:

`POST /api/admin/orders/{order_id}/recover-access`

The operator authenticates with the independent `DOKKOMPLEKT_ORDER_RECOVERY_SECRET`, supplies the checkout machine hash, and receives a newly generated access token exactly once. The store updates the token hash under the same memory write lock or PostgreSQL `FOR UPDATE` transaction. Existing tokenized orders cannot be silently rotated through this endpoint. An old order without a checkout machine binding requires the explicit `bind_missing_machine=true` support action.

## Trusted reverse proxies

`X-Forwarded-For` is ignored for direct or untrusted peers. Configure `DOKKOMPLEKT_TRUSTED_PROXY_CIDRS` with the exact proxy addresses or CIDRs that overwrite/append the header. The server then walks the chain from the trusted edge toward the client and rate-limits the nearest untrusted address. API requests arriving from a configured proxy without a valid forwarded chain fail closed when `DOKKOMPLEKT_TRUSTED_PROXY_REQUIRE_X_FORWARDED_FOR=true`.
