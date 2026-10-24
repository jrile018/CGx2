#!/usr/bin/env python3
"""Implemented region/guard/session/copy evidence. Red expectations stay red."""
import argparse
import json
from pathlib import Path
import re
import shutil
import sys
from common import HERE, ROOT, Journal, digest, normal, observed, rejection, stable
from validation import roots_ok, session_unavailable, validate_direct, matrix_inventory, SOURCE_CASES, sentinel_status
from integrity import audit


def primary(row):
    found = []
    for stream in ('stdout', 'stderr'):
        text = row.get(stream, '')
        # The CLI writes a diagnostic JSON line after its evidence JSON line.
        for line in text.splitlines():
            try:
                data = json.loads(line)
                rows = data if isinstance(data, list) else data.get('diagnostics', [data])
                found.extend((r['code'], r.get('span')) for r in rows if isinstance(r, dict) and 'code' in r)
            except (ValueError, TypeError, AttributeError):
                pass
            native = re.fullmatch(r'(CGR\d+):.*? (\d+)\.\.(\d+)', line)
            located = re.fullmatch(r'(CGR\d+):[^\n]*?:(\d+)\.\.(\d+):.*', line)
            probe = re.fullmatch(r'PRIMARY (CGR\d+) Some\(Span \{ start: (\d+), end: (\d+) \}\).*', line)
            match = native or located or probe
            if match:
                found.append((match[1], {'start': int(match[2]), 'end': int(match[3])}))
    return found[0] if len(found) == 1 else None


def byte_span(source, needle):
    text = source.read_text()
    start = len(text[:text.index(needle)].encode())
    return {'start': start, 'end': start + len(needle.encode())}


def frozen_hashes(binary):
    return {p.name: digest(p) for p in sorted(binary.iterdir()) if p.is_file()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary-dir', type=Path, required=True)
    p.add_argument('--output-dir', type=Path, required=True)
    p.add_argument('--standalone-root', type=Path, required=True,
                   help='Private writable directory outside every repository; rustc TMPDIR stays private.')
    p.add_argument('--skip-fault-native', action='store_true', help='Exploration only: required rows become unrun.')
    a = p.parse_args()
    j = Journal(a.output_dir)
    j.required_commands, j.required_checks = matrix_inventory(None)
    source_bound = None
    binary = a.binary_dir.resolve()
    before = frozen_hashes(binary)
    j.report['frozen_before'] = before
    inputs = list(HERE.glob('*.py')) + list(HERE.glob('*.rs')) + list((HERE / 'fixtures').glob('*.cg'))
    j.report['harness_inputs'] = {str(p.relative_to(HERE)): digest(p) for p in inputs}
    for source in inputs:
        snapshot=j.output/'harness-snapshot'/source.relative_to(HERE)
        snapshot.parent.mkdir(parents=True,exist_ok=True)
        snapshot.write_bytes(source.read_bytes())
    j.report['runtime_sources'] = {str(p.relative_to(ROOT)): digest(p) for p in (ROOT / 'crates/cgx2-runtime/src').glob('*.rs')}
    j.report['limits'] = {'wall_seconds': 180, 'cpu_seconds': 120, 'address_bytes': 4*1024**3,
                          'file_bytes': 512*1024**2, 'pipe_bytes_each': 2*1024**2}
    standalone = a.standalone_root.resolve() / j.output.name
    wrapper = j.output / 'rustc-two-threads'
    wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n')
    wrapper.chmod(0o700)
    fixtures = j.output / 'fixtures'
    shutil.copytree(HERE / 'fixtures', fixtures)

    def compile_probe(name, library):
        executable = j.output / name
        row = j.run(name+'-build', [wrapper, '--edition=2021', HERE/(name+'.rs'), '--extern',
                    f'{library}={binary / ("lib"+library+".rlib")}', '-L', f'dependency={binary / "deps"}', '-o', executable])
        j.check(name+'-build', normal(row))
        return executable if normal(row) else None

    def check_run(name, row, expected, reference=False, error=None, unavailable=False):
        j.check(name+'-result', normal(row, 1 if error else 0) and
                (primary(row) == error if error else row.get('stdout') == expected), expected_error=error, actual_error=primary(row))
        memory = observed(row)
        if unavailable:
            j.check(name+'-pre-session-unavailable', session_unavailable(memory))
        else:
            j.check(name+'-balanced-roots', roots_ok(memory), observation=memory)
            regions = memory.get('memory', {}).get('regions') if isinstance(memory, dict) else None
            j.check(name+'-manual-zero-collections', isinstance(regions, list) and bool(regions) and all(r.get('collection_count') == 0 for r in regions if r.get('mode') == 'manual'))
        if reference is not False:
            j.check(name+'-deterministic-parity', memory is not None and reference is not None and stable(memory) == stable(reference))
        return memory

    def targets(name, source, expected, reference, error=None, fault=None, reference_result=None):
        for mode in ('debug', 'release', 'standalone'):
            tag = f'{name}-{mode}'
            artifact = j.output / tag
            if mode == 'standalone':
                emitted = standalone / (tag+'.rs')
                emit = j.run(tag+'-emit', [binary/'cgx2c', 'emit-rust', source, '--out', emitted])
                j.check(tag+'-emit', normal(emit))
                if not normal(emit):
                    continue
                argv = [wrapper, '--edition=2021', emitted, '-o', artifact]
                extra = {}
                if fault:
                    argv += ['--cfg=feature="fault-injection"']
                    extra['CGX2_TEST_FAIL_RESERVE'] = str(fault)
                built = j.run(tag+'-build', argv, cwd=standalone, extra=extra)
                if normal(built):
                    relocated = standalone / tag
                    shutil.copy2(artifact, relocated)
                    j.report.setdefault('standalone_artifacts', {})[tag] = {
                        'source': str(emitted), 'source_sha256': digest(emitted),
                        'binary': str(relocated), 'binary_sha256': digest(relocated),
                        'embedded_runtime_sources': j.report['runtime_sources']}
                    j.save()
                    artifact = relocated
            else:
                argv = [binary/'cgx2c', 'build', source, '--out', artifact, '--rustc', wrapper]
                if mode == 'release':
                    argv.append('--release')
                if fault:
                    argv += ['--test-fail-reserve', str(fault)]
                built = j.run(tag+'-build', argv)
            j.check(tag+'-build', normal(built))
            if normal(built):
                j.report.setdefault('native_artifacts', {})[tag] = {'path':str(artifact),'sha256':digest(artifact)}
                j.save()
                actual = j.run(tag+'-run', [artifact], cwd=standalone)
                check_run(tag, actual, expected, reference, error=error, unavailable=fault == 1)
                if reference_result is not None:
                    j.check(tag+'-actual-error-parity', normal(reference_result, reference_result.get('exit')) and reference_result.get('exit') in (0, 1) and normal(actual, reference_result.get('exit')) and (reference_result.get('exit') == 0 or primary(reference_result) is not None) and primary(actual) == primary(reference_result), actual_primary=primary(actual), source_primary=primary(reference_result))

    try:
        if not audit(j, binary, 'before'):
            raise ValueError('frozen executable inputs do not match manifest')
        manifest = json.loads((binary.parent/'frozen.json').read_text())
        expected = dict(manifest['binary_sha256'], **manifest['probe_library_sha256'])
        j.check('frozen-manifest', before == expected)
        standalone.mkdir(parents=True, exist_ok=False)
        outside = j.run('standalone-outside-repository', ['git', 'rev-parse', '--is-inside-work-tree'], cwd=standalone)
        j.check('standalone-outside-repository', normal(outside, 128))
        if not normal(outside, 128):
            raise ValueError('Standalone directory must be outside every repository')
        toolroot = j.toolchain(cwd=standalone)
        toolchain_files = [toolroot/'bin/rustc'] + list((toolroot/'lib').glob('libLLVM*.so*')) + list((toolroot/'lib').glob('librustc_driver*.so'))
        toolchain_files += list((toolroot/'lib/rustlib').glob('*/lib/libstd-*'))
        j.report['toolchain_files'] = {str(p):digest(p) for p in toolchain_files if p.is_file()}
        j.save()
        probe = compile_probe('runtime_probe', 'cgx2_runtime')
        if probe:
            direct = j.run('direct-runtime-public-controls', [probe])
            j.check('direct-runtime-process', normal(direct))
            direct_rows, direct_errors, direct_bound = validate_direct(direct.get('stdout', ''))
            j.report['direct_inventory'] = {'bound': direct_bound, 'errors': direct_errors, 'rows': direct_rows}
            j.check('direct-runtime-inventory', not direct_errors, errors=direct_errors)
            if direct_errors:
                j.report['incomplete_reason'] = 'invalid direct-runtime inventory'
            seen = set()
            for row in direct_rows:
                if row['name'] not in seen:
                    j.check('direct:'+row['name'], row['passed'], evidence_kind='direct-library', details=row['details'])
                    seen.add(row['name'])
        for name in SOURCE_CASES:
            source = fixtures/(name+'.cg')
            checked = j.run(name+'-check', [binary/'cgx2', '--diagnostic-format=json', 'check', source])
            j.check(name+'-check', normal(checked))
            if not normal(checked):
                continue
            code = {'arithmetic':'CGR006','contract':'CGR015','failed-argument':'CGR006'}.get(name)
            error = (code, byte_span(source, 'false' if name == 'contract' else '(1 / 0)' if name == 'arithmetic' else '1 / 0')) if code else None
            result = j.run(name+'-interpreter', [binary/'cgx2', '--diagnostic-format=json', 'run', source])
            expected_value = '2\n' if name == 'region' else '42\n'
            memory = check_run(name+'-interpreter', result, expected_value, error=error)
            targets(name, source, expected_value, memory, error=error)
        # Exact static negatives and preserved native/emission sentinels.
        for name, needle in [('cross-region','xs):'), ('cross-region-utf8','xs):'),
                             ('fallback-resource','memory.default_region()')]:
            source = fixtures/(name+'.cg')
            span = byte_span(source, needle)
            if needle == 'xs):':
                span['end'] = span['start']+2
            rejected = j.run(name+'-negative', [binary/'cgx2', '--diagnostic-format=json', 'check', source])
            j.check(name+'-exact-code-span', rejection(rejected, 'CGT095', span))
            for operation in ('build', 'emit-rust', 'emit-python'):
                sentinel = j.output/(name+'-'+operation+'-sentinel')
                sentinel.write_bytes(b'prior artifact\n')
                rejected = j.run(name+'-'+operation, [binary/'cgx2c', operation, source, '--out', sentinel, '--force'])
                rejected_ok, preserved = sentinel_status(rejected, sentinel, b'prior artifact\n', 'CGT095')
                j.check(name+'-'+operation+'-rejected', rejected_ok)
                j.check(name+'-'+operation+'-preserved', preserved)
        # A dependency-only region escape: exact file attribution remains required.
        package = j.output/'dependency-negative'
        package.mkdir()
        (package/'cgx2.toml').write_text('schema_version = 1\nname = "negative"\nentry = "main.cg"\nsources = ["main.cg", "lib.cg"]\n')
        (package/'main.cg').write_text('module app.main\nimport app.lib\nfn main() -> i64 effects(ManagedAlloc):\n    app.lib.fail()\n')
        dependency = package/'lib.cg'
        dependency.write_text('module app.lib\n# café 🔧\n'+(fixtures/'cross-region.cg').read_text().replace('fn main()', 'pub fn fail()'))
        span = byte_span(dependency, 'xs):'); span['end']=span['start']+2
        negative = j.run('dependency-negative', [binary/'cgx2','--diagnostic-format=json','check',package])
        j.check('dependency-negative-exact-code-span', rejection(negative,'CGT095',span))
        j.check('dependency-negative-exact-file', rejection(negative,'CGT095',span,'lib.cg'),
                expected_file='lib.cg', required_gate='known missing static dependency-file attribution')
        for operation in ('build','emit-rust','emit-python'):
            sentinel=j.output/('dependency-'+operation+'-sentinel'); sentinel.write_bytes(b'prior artifact\n')
            row=j.run('dependency-'+operation,[binary/'cgx2c',operation,'--package',package,'--out',sentinel,'--force'])
            rejected_ok, preserved = sentinel_status(row, sentinel, b'prior artifact\n', 'CGT095')
            j.check('dependency-'+operation+'-rejected', rejected_ok)
            j.check('dependency-'+operation+'-preserved', preserved)
        probe = compile_probe('source_probe', 'cgx2_cli')
        if probe:
            source=fixtures/'sweep.cg'
            checked=j.run('sweep-source-admission',[binary/'cgx2','--diagnostic-format=json','check',source])
            j.check('sweep-source-admission',normal(checked))
            rows=[]
            for index in range(1,65):
                row=j.run(f'source-reserve-{index}',[probe,source,str(index)])
                value=row.get('stdout','')
                # All error arms are copied scalars; no hidden allocations/extra failure paths.
                allowed={'42\n','0\n','-10\n','-1\n','-2\n','-3\n','-4\n','-5\n'}
                j.check(f'source-reserve-{index}-fixed-outcome',normal(row) and value in allowed)
                memory=check_run(f'source-reserve-{index}',row,value,unavailable=index==1)
                rows.append((row,memory))
            failures=[i+1 for i,(r,_) in enumerate(rows) if r.get('stdout')!='42\n']
            bound=max(failures,default=0)
            source_bound = bound
            j.required_commands, j.required_checks = matrix_inventory(bound)
            j.report['source_reserve_bound']={'ceiling':64,'last_failure':bound,
                'rationale':'Straight-line two-region/nested-guard/copy fixture; failure arms are scalar-only. All 64 indices run; unaffected tail must match baseline. No general-program bound claimed.'}
            j.check('source-sweep-has-unaffected-tail',0<bound<64 and all(normal(r) and r['stdout']=='42\n' and stable(m)==stable(rows[-1][1]) for r,m in rows[bound:]))
            if a.skip_fault_native:
                j.report['incomplete_reason'] = 'required compiled sweep skipped'
                j.report['unrun_fault_native']=list(range(1,bound+2))
                j.check('required-fault-native-run',False,disposition='unrun exploration option')
            else:
                for index in range(1,bound+2):
                    row,memory=rows[index-1]
                    targets(f'source-fault-{index}',source,row.get('stdout'),memory,fault=index,reference_result=row)
        j.report['unsupported_required']=[
            'Actual Session identifier exhaustion (no fabricated counter-exhaustion claim)',
            'Global host allocator OOM, process abort/kill, forgotten scopes: no cleanup guarantee',
            'General aggregate ownership, partial moves, projected loans/reborrows, recursive/generic/package ownership',
            'Source copy_owned/promote with readable/retryable same-owner failure and full v3 authentication',
            'Leak-enabled sanitizer, independent audit and broader R01-R15 external release gates']
    except Exception:
        j.exception()
    finally:
        audit(j, binary, 'after')
        after = frozen_hashes(binary)
        j.report['frozen_after']=after
        j.check('frozen-unchanged', before==after)
        j.check('harness-inputs-unchanged',j.report['harness_inputs']=={str(p.relative_to(HERE)):digest(p) for p in inputs})
    return j.finish()


if __name__=='__main__':
    raise SystemExit(main())
