#!/usr/bin/env python3
"""Observe an exact recorded compiler build with an independently identified harness."""
import argparse
import json
import os
from pathlib import Path
from common import HERE, Journal, digest
from integrity import BINARIES, LIBRARIES, harness_paths, helper_paths, freeze_inputs, audit
from provenance import checkout_identity, recorded_build


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verification-dir', type=Path, required=True)
    parser.add_argument('--build-invocation', type=Path, required=True)
    parser.add_argument('--build-source-commit', required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    j = Journal(args.output_dir)
    j.required_checks = {'recorded-build', 'compiler-inputs-unchanged', 'frozen-inputs-finalized'}
    try:
        prior = args.verification_dir.resolve()
        build = recorded_build(prior, args.build_invocation, args.build_source_commit)
        j.report['build_identity'] = build
        j.check('recorded-build', True)
        j.check('compiler-inputs-unchanged', True)
        toolroot = j.toolchain()
        toolchain = {Path(p) for p in json.loads((prior / 'inputs.json').read_text())['toolchain']}
        if toolroot / 'bin/rustc' not in toolchain:
            raise ValueError('current sysroot differs from original build')
        for directory in j.env.get('LIBRARY_PATH', '').split(':'):
            if directory:
                toolchain.update(p for p in Path(directory).iterdir() if p.is_file())
        frozen = j.output / 'frozen'
        frozen.mkdir()
        for name in BINARIES + LIBRARIES:
            # Exact canonical inodes and original dependency tree; no old writes.
            os.link(prior / 'frozen' / name, frozen / name)
        (frozen / 'deps').symlink_to((prior / 'frozen/deps').resolve(), target_is_directory=True)
        candidate = {'schema_version': 1, 'build': build, 'observing_harness': checkout_identity()}
        (j.output / 'candidate.json').write_text(json.dumps(candidate, indent=2) + '\n')
        original = json.loads((prior / 'frozen.json').read_text())
        manifest = {'schema_version': 3, 'source_commit': build['source_commit'],
                    'harness_snapshot_sha256': candidate['observing_harness']['harness_snapshot_sha256'],
                    'candidate_sha256': digest(j.output / 'candidate.json'),
                    'binary_directory': str(frozen), 'binary_sha256': original['binary_sha256'],
                    'probe_library_sha256': original['probe_library_sha256']}
        (j.output / 'frozen.json').write_text(json.dumps(manifest, indent=2) + '\n')
        (j.output / 'source.json').write_text(json.dumps(build['compiler'], indent=2) + '\n')
        (j.output / 'inputs.json').write_text(json.dumps(freeze_inputs(frozen, toolchain), indent=2) + '\n')
        for path in harness_paths():
            destination = j.output / 'harness-snapshot' / path.relative_to(HERE)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(path.read_bytes())
        from common import ROOT
        for path in helper_paths():
            destination = j.output / 'helper-snapshot' / path.relative_to(ROOT)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(path.read_bytes())
        audit(j, frozen, 'finalized')
    except Exception:
        j.exception()
    return j.finish()


if __name__ == '__main__':
    raise SystemExit(main())
