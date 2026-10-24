import gzip,hashlib,json,shutil,sys
from pathlib import Path
root=Path('target/ordinary-harness-finalization').resolve()
out=Path('crates/cgx2-syntax/ordinary-parity-repair-evidence/finalization-evidence/setup-and-audits-opaque-1').resolve()
out.mkdir(parents=True,exist_ok=False)
(out/'packager.py').write_text(sys.orig_argv[-1])
def hash_bytes(data):return hashlib.sha256(data).hexdigest()
files={}
def emit(path,relative):
 data=path.read_bytes();dest=out/(str(relative)+'.gz');dest.parent.mkdir(parents=True,exist_ok=True)
 with dest.open('wb')as raw:
  with gzip.GzipFile(filename='',fileobj=raw,mode='wb',mtime=0)as gz:gz.write(data)
 files[str(relative)+'.gz']={'original_path':str(path),'bytes':len(data),'sha256':hash_bytes(data),'gzip':{'bytes':dest.stat().st_size,'sha256':hash_bytes(dest.read_bytes())}}
for path in sorted(root.iterdir()):
 if path.is_file():emit(path,Path('driver')/path.name)
 elif path.is_dir() and (path/'report.json').is_file():
  for source in sorted(path.rglob('*')):
   if source.is_file() and 'frozen' not in source.relative_to(path).parts:emit(source,Path('attempts')/source.relative_to(root))
(out/'manifest.json').write_text(json.dumps({'schema_version':2,'opaque_raw_evidence':True,'packager_sha256':hash_bytes((out/'packager.py').read_bytes()),'files':files},indent=2)+'\n')
(out/'summary.json').write_text(json.dumps({'archives':len(files),'interpretation':'Opaque compression preserves malformed and partial reports verbatim; no product acceptance inferred.'},indent=2)+'\n')
print(json.dumps({'archives':len(files)}))
