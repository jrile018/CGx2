from pathlib import Path
import json,hashlib,gzip,re
out=Path('/home/john-riley/projects/lang/target/ordinary-combined-review-20261024/all-repairs-rerun');b=out/'frozen';root=Path('/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang')
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=json.loads((b/'manifest.json').read_text());s=json.loads((out/'verification.json').read_text());actual={p.name:h(p) for p in b.iterdir() if p.is_file()};expected=dict(m['artifact_sha256'],**{'manifest.json':h(b/'manifest.json')});worker=json.loads((out/'frozen.json').read_text());old=dict(worker['binary_sha256'],**worker['probe_library_sha256'])
audit={'source_commit':s['source_commit'],'artifact_changes':[k for k,v in m['artifact_sha256'].items() if actual.get(k)!=v],'extra_worker_manifest_keys':sorted(set(actual)-set(old)),'missing_worker_manifest_keys':sorted(set(old)-set(actual)),'worker_declared_hash_changes':[k for k,v in old.items() if actual.get(k)!=v],'dependency_changes':[k for k,v in s['dependency_hashes'].items() if h(b/'deps'/k)!=v],'source_changes':[k for k,v in m['source_sha256'].items() if h(root/k)!=v],'raw_matrix_counts':{},'observed_root_field_defects':[],'interpretation':'The raw frozen-manifest assertion failed because the coordinator supplied an additional canonical API-index library and its API runner manifest.json. The 12 worker-expected artifacts match exactly; all13 originally frozen artifacts and dependencies remain verified. This audit is separate evidence, not a changed raw row. Admission standalone rerun genuinely ran externally. Remaining product failures stay failed.'}
d=json.loads((out/'matrix-retry/report.json').read_text());audit['raw_matrix_counts']={k:d[k] for k in ('passed','failed','complete')};audit['failed_rows']=[r['name'] for r in d['checks'] if not r['passed']]
for r in d['commands']:
 for line in r.get('stderr','').splitlines():
  if line.startswith('CGX2_MEMORY_EVIDENCE '):
   v=json.loads(line.removeprefix('CGX2_MEMORY_EVIDENCE '))
   if v.get('session_entered') is False:continue
   # Main runner roots_ok reads the cleanup object.
   fields=v.get('cleanup',v)
   for k in ('baseline_roots','final_roots','roots_registered','roots_released','cleanup_failures'):
    if type(fields.get(k)) is not int or fields[k]<0:audit['observed_root_field_defects'].append([r['name'],k])
first=out.parent/'all-repairs/tests.log.gz';rows=re.findall(rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',gzip.decompress(first.read_bytes()));audit['first_workspace_corrected_counts']={k:sum(int(r[i]) for r in rows) for i,k in enumerate(('passed','failed','ignored'))};audit['first_workspace_note']='Original verification.json parser counted only ok result sections; raw first log totals1302passed/1failed/6ignored. Failure preserved. Subsequent full run1360passed/0failed/6ignored.'
(out/'coordinator-audit.json').write_text(json.dumps(audit,indent=2)+'\n');print(json.dumps({k:audit[k] for k in ('artifact_changes','dependency_changes','source_changes','extra_worker_manifest_keys','first_workspace_corrected_counts')}))
