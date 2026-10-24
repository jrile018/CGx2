#!/usr/bin/env python3
"""Export immutable raw evidence; never export built binaries into Git."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import subprocess
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
def digest(data):return hashlib.sha256(data).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--scratch-dir',type=Path,required=True)
    p.add_argument('--verification-dir',type=Path,required=True)
    p.add_argument('--product-dir',type=Path,required=True)
    p.add_argument('--standalone-dir',type=Path,required=True)
    p.add_argument('--output-dir',type=Path,required=True)
    p.add_argument('--regression-product-dir',type=Path,help='Reuse an existing regression replay only when frozen artifact hashes match exactly.')
    a=p.parse_args();scratch=a.scratch_dir.resolve();verify=a.verification_dir.resolve();product=a.product_dir.resolve();standalone=a.standalone_dir.resolve();out=a.output_dir.resolve()
    out.mkdir(parents=True,exist_ok=False)
    regression_product=a.regression_product_dir.resolve() if a.regression_product_dir else product
    prod=json.loads((product/'product.json').read_text())
    regression=json.loads((regression_product/'product.json').read_text())
    specification=json.loads((ROOT/'plans/release-completion/s2b-ordinary-memory/regressions.json').read_text())
    regression_commands={r['name']:r for r in regression['commands'] if r['name'].startswith('regression-')}
    regression_checks=[r for r in regression['checks'] if r['name'].startswith('regression-') or r['name']=='historical-failure-is-only-the-unchanged-shadow-negative']
    regressions_valid=all((r:=regression_commands.get('regression-'+row['name'])) and r['reason']=='exit' and r['exit']==row['expected_exit'] and r.get('signal') is None for row in specification['checks'])
    regressions_valid &= len(regression_checks)==len(specification['checks'])+1 and all(c['passed'] for c in regression_checks)
    regressions_valid &= regression['frozen_before']==regression['frozen_after']==prod['frozen_before']==prod['frozen_after']
    original=json.loads((regression_product/'regressions/owned-result-original/owned-result-original.json').read_text())
    failures=[r for r in original.get('commands',[]) if r.get('passed') is False]
    regressions_valid &= original.get('passed')==523 and original.get('failed')==1 and len(failures)==1 and failures[0].get('label')=='shadow-builtin:reject' and failures[0].get('reason')=='exit' and failures[0].get('exit')==0 and failures[0].get('signal') is None
    own_regression_checks=[r for r in prod['checks'] if r['name'].startswith('regression-') or r['name']=='historical-failure-is-only-the-unchanged-shadow-negative']
    bounded=[r for r in prod['checks'] if r not in own_regression_checks]
    bounded_passed=sum(c['passed'] for c in bounded);bounded_failed=sum(not c['passed'] for c in bounded)
    regression_passed=sum(c['passed'] for c in regression_checks);regression_failed=sum(not c['passed'] for c in regression_checks)
    manifest={}
    def export(path,relative):
        if not path.is_file():return
        data=path.read_bytes();target=out/(str(relative)+'.gz');target.parent.mkdir(parents=True,exist_ok=True)
        encoded=gzip.compress(data,mtime=0);target.write_bytes(encoded)
        manifest[str(target.relative_to(out))]={'bytes':len(data),'sha256':digest(data),'gzip_sha256':digest(encoded)}
    for path in sorted(scratch.iterdir()):
        if path.is_file() and path.suffix in ['.log','.json','.txt','.cg']:
            export(path,Path('supervision')/path.name)
    for d in sorted(scratch.glob('verification-*')):
        for path in d.glob('*'):
            if path.is_file() and path.suffix in ['.log','.json']:export(path,Path('quality')/d.name/path.name)
    for d in sorted(scratch.glob('product-*')):
        if not d.is_dir():continue
        name=d.name
        for filename in ['product.json','runner-snapshot.py']:export(d/filename,Path('products')/name/filename)
        for path in (d/'fixtures-snapshot').glob('*.cg'):export(path,Path('products')/name/'fixtures'/path.name)
        for path in (d/'package-snapshot').glob('*'):export(path,Path('products')/name/'package'/path.name)
    for path in (regression_product/'regressions').glob('**/*.json'):export(path,Path('regressions')/path.relative_to(regression_product/'regressions'))
    for path in product.glob('outside/*.rs'):export(path,Path('standalone-source')/path.name)
    for d in sorted(scratch.glob('standalone-*')):
        if d.is_dir():
            for path in d.glob('*.json'):export(path,Path('standalone-attempts')/d.name/path.name)
    export(standalone/'standalone.json','standalone-outside-repository.json')
    export(verify/'compiler-source.json','compiler-source.json')
    for path in (scratch/'candidate-2-source').glob('*.rs'):export(path,Path('candidate-2-source')/path.name)
    quality=json.loads((verify/'verification.json').read_text());prod=json.loads((product/'product.json').read_text());separate=json.loads((standalone/'standalone.json').read_text());frozen=json.loads((verify/'frozen.json').read_text())
    source=json.loads((verify/'compiler-source.json').read_text());changed=[name for name,sha in source['files'].items() if digest((ROOT/name).read_bytes())!=sha]
    text=(verify/'workspace.log').read_text();counts=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',text)]
    root_hashes={};root_unchanged=True
    for name in ['Cargo.toml','Cargo.lock']:
        current=(ROOT/name).read_bytes();baseline=subprocess.check_output(['git','show','0d321d3d1b0341121b8363a5a87eeaf674c20eec:'+name],cwd=ROOT)
        root_hashes[name]=digest(current);root_unchanged &= current==baseline
    report={'schema_version':1,'status':'incomplete-ordinary-memory-checkpoint','reviewed_base':'0d321d3d1b0341121b8363a5a87eeaf674c20eec','ordinary_memory_complete':False,'release_complete':False,'consumer_fanout_authorized':False,
        'workspace':{'passed':sum(r[0] for r in counts),'failed':sum(r[1] for r in counts),'ignored':sum(r[2] for r in counts)},'quality_commands':quality['commands'],
        'products':{'new_and_regressions':{'passed':bounded_passed+regression_passed,'failed':bounded_failed+regression_failed},'bounded_product':{'passed':bounded_passed,'failed':bounded_failed},'regression_replay':{'checks_passed':regression_passed,'checks_failed':regression_failed,'commands':len(regression_commands),'validated':regressions_valid,'report':str(regression_product/'product.json'),'sha256':digest((regression_product/'product.json').read_bytes()),'reused_same_artifacts':regression_product!=product,'parent_report_failed_checks_preserved':regression['failed']},'outside_repository_standalone':{'passed':separate['passed'],'failed':separate['failed'],'working_directory':separate['working_directory']}},
        'regression_commands':list(regression_commands.values()),
        'historical_negative':'Unchanged original owned-Result runner523 passed/1 obsolete shadow-builtin failure; reconciled593/restored17 run separately.',
        'frozen_manifest':frozen,'frozen_artifacts_changed':[n for n in prod['frozen_before'] if prod['frozen_before'][n]!=prod['frozen_after'][n]],'compiler_source_sha256':source['aggregate_sha256'],'changed_compiler_inputs_since_freeze':changed,
        'root_manifests_unchanged':root_unchanged,'root_manifest_sha256':root_hashes,'root_lock_reconciliation_required':False,'harness_selftests':8,
        'known_attempts':[
            'Initial restricted workspace run failed legacy routing and local fixture permissions; all logs retained.',
            'Exploratory product64/15: missing Mutate fixture declaration, diagnostic stream handling and incoherent probe library snapshot.',
            'Exploratory product86/25: probe Debug bound and preexec CPU hard-limit setup failure; no false pass counted.',
            'Candidate2 product111/0 and regressions passed, then audit reproduced forbidden default-region access in failed Session fallback.',
            'Candidate4 product114/0 and outside-repository standalone19/0 passed before additive interface/package changes.',
            'Interface attempts retained effect-fixture and imported-name resolution failures; the initial compile failure raw log was overwritten (explicit evidence limitation).',
            'Standalone8 hit /tmp quota with linker failure; copy startup attempts9/10 failed under the same quota. Superseded generated ELF files were removed; reports/source/frozen compiler files retained. A first cleanup audit write also failed; outside cleanup hashes were saved before deletion.',
            'Product7 passed all18 regressions but retained six incorrect package-argv failures. Product8 fixes --package against unchanged artifacts; successful same-artifact regression evidence is selected explicitly.',
            'Verification5 passed tests/Clippy but caught unformatted AST checks; verification6 caught an overbroad rejection of parser comparison helpers. Verification7 supersedes both.',
            'Current source adds per-function transitive resource effects, AST/source-bundle provenance, v2 API metadata and a distinct checked memory package certificate.'
        ],'missing_required_rows':prod['missing_required_rows'] + ['complete exploratory evidence retention: first interface-sema failure log was overwritten'],
        'exploratory_evidence_limitation':json.loads((scratch/'interface-command-history.json').read_text())['evidence_limitation'] if (scratch/'interface-command-history.json').is_file() else 'Historical checkpoint lost its first interface compile failure raw log; see committed acceptance ledger.',
        'external_gates':'Leak-enabled checks and independent audit unrun; no disabled-leak result is treated as acceptance. Full R01-R15 release requirements remain open.',
        'evidence_manifest':'manifest.json'}
    (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');(out/'acceptance.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'workspace':report['workspace'],'products':report['products'],'ordinary_memory_complete':False,'changed_compiler_inputs':changed}))
    return int(bool(changed) or not root_unchanged or bool(report['frozen_artifacts_changed']) or bool(bounded_failed) or not regressions_valid or bool(separate['failed']))
if __name__=='__main__':raise SystemExit(main())
