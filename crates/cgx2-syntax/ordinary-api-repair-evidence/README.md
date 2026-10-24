# Ordinary public API repair evidence

See ../S2B-ORDINARY-API-REPAIR.md for scope, exact results, failed attempts,
version semantics, remaining limitations and reproduction instructions.

command.py retains bounded attempts. freeze.py freezes completed builds.
run.py tests the frozen public view and checked negatives. replay.py preserves
historical scripts while selecting private working directories. finalize.py
checks identities and exports retained artifacts. Attempt/output names are
unique; earlier evidence is never replaced.

Raw command records and generated sources are gzip-compressed in evidence/.
Final product assertions are in evidence/product-final-2/report.json; the first
attempt is retained separately. Native artifacts remain in private target
directories and are identified by SHA-256 in committed manifests.
