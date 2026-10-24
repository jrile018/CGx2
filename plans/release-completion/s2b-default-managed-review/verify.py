import argparse,gzip,hashlib,json,os,resource,sys,re
from pathlib import Path
ap=argparse.ArgumentParser()
ap.add_argument("--source",type=Path,required=True)
ap.add_argument("--output",type=Path,required=True)
ap.add_argument("--only")
args=ap.parse_args()
root=args.source.resolve();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
sys.path.insert(0,str(root/"crates/cgx2-syntax/owned-result-runner-evidence"))
from supervisor import supervise,evidence
old="/home/john-riley/.codex/worktrees/685c/lang"
target=Path("/home/john-riley/projects/lang/target/release-plan-review")
temporary=Path("/home/john-riley/projects/lang/target/cgx2-wave21-tmp")
target.mkdir(parents=True,exist_ok=True);temporary.mkdir(parents=True,exist_ok=True)
env={**os.environ,"RUSTUP_TOOLCHAIN":"1.98.1","CARGO_BUILD_JOBS":"2","CARGO_TARGET_DIR":str(target),"TMPDIR":str(temporary),"LIBRARY_PATH":"/tmp/cgx2-wave22-lib","PYTHONDONTWRITEBYTECODE":"1"}
delivery=json.loads((root/"crates/cgx2-syntax/default-managed-evidence/summary.json").read_text())
report={"source_commit":"bc4ba95e710eaed3816291fdc67de1478e0456f5","source_root":str(root),"checks":[],"release_complete":False,"external_evidence":"unrun; worker leak negative controls remain blocked","environment":{k:env[k] for k in ["RUSTUP_TOOLCHAIN","CARGO_BUILD_JOBS","CARGO_TARGET_DIR","TMPDIR","LIBRARY_PATH"]}}
def limits():
 resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3))
 resource.setrlimit(resource.RLIMIT_CPU,(900,900))
for check in delivery["checks"]:
 name=check["name"]
 if name.startswith("asan-") or (args.only and name not in args.only.split(",")):continue
 check_out=out/name;check_out.mkdir(exist_ok=True)
 original_evidence=str(Path(check["evidence"]).parent)
 replacements=[(old+"/target/release-default-managed",str(target)),(old+"/crates/cgx2-syntax/default-managed-evidence/"+original_evidence,str(check_out)),(old,str(root))]
 cmd=[]
 for arg in check["command"]:
  for src,dest in replacements:arg=arg.replace(src,dest)
  cmd.append(arg)
 print("start",name,flush=True)
 result=supervise(cmd,cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=900,merged=True)
 data=result["stdout"];log=out/(name+".log.gz");log.write_bytes(gzip.compress(data,mtime=0))
 record={"name":name,"command":cmd,**evidence(result,include_output=False),"log":log.name,"log_sha256":hashlib.sha256(data).hexdigest(),"status":"passed" if result["exit"]==0 and result["reason"]=="exit" else "failed"}
 if name=="owned-result-original" and result["exit"]==1 and result["reason"]=="exit" and b"shadow-builtin:reject" in data:record["status"]="known-superseded-negative"
 if name=="workspace-tests":
  rows=re.findall(rb"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored",data)
  record.update({label:sum(int(r[i]) for r in rows) for label,i in [("passed",1),("failed",2),("ignored",3)]})
 report["checks"].append(record);(out/"verification.json").write_text(json.dumps(report,indent=2)+"\n")
 print("finish",name,record["status"],result["exit"],flush=True)
 if name=="build" and record["status"]!="passed":break
sys.exit(int(any(x["status"]=="failed" for x in report["checks"])))
