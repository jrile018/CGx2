#!/usr/bin/env python3
"""Finalize a verified local build after harness edits; this command never runs Cargo."""
import argparse
import json
from pathlib import Path
import shutil
from common import HERE, ROOT, Journal, digest, normal
from integrity import BINARIES, LIBRARIES, compiler_paths, harness_paths, freeze_inputs, audit


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verification-dir',type=Path,required=True)
    parser.add_argument('--output-dir',type=Path,required=True)
    args=parser.parse_args();j=Journal(args.output_dir)
    j.required_checks={'recorded-build','compiler-inputs-unchanged','source-base','frozen-inputs-finalized'}
    try:
        prior=args.verification_dir.resolve()
        report=json.loads((prior/'report.json').read_text())
        build=[r for r in report['commands'] if r['name']=='build']
        j.report['build_reference']={'path':str(prior/'report.json'),'sha256':digest(prior/'report.json'),
                                     'workspace_checks':report['checks']}
        j.check('recorded-build',len(build)==1 and normal(build[0]))
        if len(build)!=1 or not normal(build[0]):raise ValueError('no successful unique candidate build')
        original=json.loads((prior/'source.json').read_text())
        current={str(p.relative_to(ROOT)):digest(p) for p in compiler_paths()}
        j.check('compiler-inputs-unchanged',current==original)
        if current!=original:raise ValueError('compiler inputs changed since candidate build')
        source=j.run('source-identity',['git','rev-parse','HEAD'])
        j.check('source-base',normal(source) and source.get('stdout','').strip()==j.report['reviewed_base'])
        j.run('source-diff',['git','diff','--'])
        sysroot=j.run('toolchain-sysroot',['rustc','--print','sysroot'])
        identity=j.run('toolchain-identity',['rustc','--version','--verbose'])
        if not normal(sysroot) or not normal(identity) or 'rustc 1.98.1' not in identity.get('stdout',''):
            raise ValueError('required Rust1.98.1 identity unavailable')
        toolroot=Path(sysroot['stdout'].strip())
        toolchain={toolroot/'bin/rustc',toolroot/'bin/cargo'}
        toolchain.update((toolroot/'lib').glob('libLLVM*.so*'))
        toolchain.update((toolroot/'lib').glob('librustc_driver*.so'))
        toolchain.update(p for p in (toolroot/'lib/rustlib').glob('*/lib/*') if p.is_file())
        for directory in j.env.get('LIBRARY_PATH','').split(':'):
            if directory:toolchain.update(p for p in Path(directory).iterdir() if p.is_file())
        old=json.loads((prior/'frozen.json').read_text())
        frozen=j.output/'frozen';frozen.mkdir()
        manifest={'schema_version':2,'source_commit':source['stdout'].strip(),
                  'build_report_sha256':digest(prior/'report.json'),'binary_directory':str(frozen),
                  'binary_sha256':{},'probe_library_sha256':{}}
        for name in BINARIES+LIBRARIES:
            key='probe_library_sha256' if name.endswith('.rlib') else 'binary_sha256'
            if digest(prior/'frozen'/name)!=old[key][name]:raise ValueError('changed canonical artifact: '+name)
            shutil.copy2(prior/'frozen'/name,frozen/name)
            manifest[key][name]=digest(frozen/name)
        (frozen/'deps').symlink_to((prior/'frozen/deps').resolve(),target_is_directory=True)
        (j.output/'frozen.json').write_text(json.dumps(manifest,indent=2)+'\n')
        (j.output/'source.json').write_text(json.dumps(current,indent=2)+'\n')
        (j.output/'inputs.json').write_text(json.dumps(freeze_inputs(frozen,toolchain),indent=2)+'\n')
        for path in harness_paths():
            destination=j.output/'harness-snapshot'/path.relative_to(HERE)
            destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(path.read_bytes())
        audit(j,frozen,'finalized')
    except Exception:
        j.exception()
    return j.finish()


if __name__=='__main__':raise SystemExit(main())
