# Fixture only

The digest-named sibling directory is a tiny verifiable **format fixture**.
Its source/tool/environment records are explicit fixture markers, its compiler
is null, and its owner job is blocked without running anything. No approval,
participant, model call or safety observation is invented. Every release gate
remains blocked/unrun. Never use this fixture as release evidence.

Run `python3 bench/fixtures/release-evidence/v1/generate.py` to reproduce it
in a fresh directory (an existing identical bundle is deliberately refused).
Use `release_evidence.py verify --bundle PATH` and `report --bundle PATH`
on its content address. The real collector captures actual bytes and host
observations instead of these fixture markers.
