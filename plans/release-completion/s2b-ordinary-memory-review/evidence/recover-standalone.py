from pathlib import Path
import hashlib,json,tarfile,shutil,os,subprocess,gzip,sys,resource
out=Path(__file__).parent
scratch=Path('/tmp/cgx2-ordinary-memory-standalone-r8xqkpnv')
audit={'scratch':str(scratch),'files':[],'process_references':[]}
for p in scratch.rglob('*'):
    if p.is_file(): audit['files'].append({'path':str(p.relative_to(scratch)),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
for process in Path('/proc').iterdir():
    if not process.name.isdigit():continue
    for link in [process/'cwd',process/'exe',*list((process/'fd').glob('*'))]:
        try: target=link.resolve()
        except (OSError,RuntimeError):continue
        if target==scratch or scratch in target.parents:audit['process_references'].append(str(link))
assert not audit['process_references'],audit['process_references']
archive=out/'failed-standalone-scratch.tar.gz'
with tarfile.open(archive,'w:gz') as tar:tar.add(scratch,arcname=scratch.name)
with tarfile.open(archive,'r:gz') as tar:
    for item in audit['files']:
        raw=tar.extractfile(scratch.name+'/'+item['path']).read()
        assert hashlib.sha256(raw).hexdigest()==item['sha256']
audit['archive_sha256']=hashlib.sha256(archive.read_bytes()).hexdigest()
(out/'failed-standalone-scratch-audit.json').write_text(json.dumps(audit,indent=2)+'\n')
shutil.rmtree(scratch)
audit['removed_after_verified_archive']=True
(out/'failed-standalone-scratch-audit.json').write_text(json.dumps(audit,indent=2)+'\n')
print('Preserved and removed own inactive scratch',sum(x['bytes'] for x in audit['files']),flush=True)
