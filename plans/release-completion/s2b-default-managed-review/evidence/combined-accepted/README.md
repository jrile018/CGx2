# Combined default-managed repair acceptance

See acceptance.json. Rust/compiler source b0f1ccc; only two Python evidence files changed in0949d5c. All1290 workspace tests and quality gates passed before final build/freeze. The full678 repair matrix and369 product checks then passed against unchanged frozen artifacts, as did116 core and87 equality outcomes and all current regression suites.16 harness selftests pass.

The original setup failure and original superseded negative are retained separately. JSON reports and their complete command stdout/stderr are losslessly gzip-exported; outer logs are retained. export-sha256.json authenticates these retained artifacts. Compiled executables remain in the private local evidence directory recorded in reports; they are not committed. Tracked source inventory and final binary/probe-library digests are included. verify-*.py are the exact executed coordinator orchestration scripts; use a new output directory when reproducing, never overwrite historical attempts.

This accepts the bounded default managed checkpoint, not complete ordinary memory or any external release gate. No release or main merge.
