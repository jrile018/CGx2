import importlib.util,json,os,shutil,tempfile
from pathlib import Path
checkout=Path.cwd()
spec=importlib.util.spec_from_file_location('checker',checkout/'tools/docs-check/check.py');c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
e=json.load(open('plans/release-completion/s1-review/evidence/s1-docs-acceptance.json'))
with tempfile.TemporaryDirectory(prefix='cgx2-docs-s1-',dir='/var/tmp') as scratch:
 root=Path(scratch);(root/'bin').mkdir();(root/'work').mkdir()
 shutil.copytree(checkout/'docs/first-release/fixtures',root/'fixtures')
 shutil.copy2(checkout/'docs/first-release/expectations.json',root/'expectations.json')
 a=['--suite',str(root/'expectations.json'),'--report',str(root/'report.json'),'--work-root',str(root/'work'),'--runnable-only']
 for name,info in e['tools'].items():
  path=Path(info['path'])
  if name!='rustc':shutil.copy2(path,root/'bin'/path.name);path=root/'bin'/path.name
  a+=['--tool',name+'='+str(path),'--pin',name+'='+info['expected_sha256']]
 a+=['--tool','python=/usr/bin/python3.14','--pin','python=52e0a13e60a981d8c4b6478be2ba5176f69da07948a056bf49cf6f077e30cb41']
 os.chdir(root)
 status=c.main(a)
 shutil.copy2(root/'report.json',checkout/'target/repair-docs-current/detached.json')
 print('outside_checkout',root,'exit',status)
 raise SystemExit(status)
