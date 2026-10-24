from pathlib import Path
import os,json,sys,gzip,hashlib,resource,subprocess
root=Path("/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang")
out=Path("/home/john-riley/projects/lang/target/managed-repairs-review-20261024/runtime");out.mkdir(parents=True,exist_ok=True)
target=Path("/home/john-riley/projects/lang/target/release-plan-review");tmp=Path("/home/john-riley/projects/lang/target/cgx2-wave21-tmp")
sys.path.insert(0,str(root/"crates/cgx2-syntax/owned-result-runner-evidence"))
from supervisor import supervise,evidence
env={**os.environ,"RUSTUP_TOOLCHAIN":"1.98.1","CARGO_BUILD_JOBS":"2","CARGO_TARGET_DIR":str(target),"TMPDIR":str(tmp),"LIBRARY_PATH":"/tmp/cgx2-wave22-lib","PYTHONDONTWRITEBYTECODE":"1"}
checks=[("build",["cargo","build","--offline","--locked","--all-features","-p","cgx2-cli","-p","cgx2c"]),("runtime-equality-tests",["cargo","test","--offline","--locked","--all-features","-p","cgx2-runtime","--test","managed_result_equality"]),("typed-equality-tests",["cargo","test","--offline","--locked","--all-features","-p","cgx2-sema","--test","managed_result_equality"]),("equality-product",[sys.executable,"crates/cgx2-syntax/managed-equality-repair-evidence/run.py","--binary-dir",str(target/"debug"),"--output-dir",str(out/"parity")])]
report={"source_commit":subprocess.check_output(["git","rev-parse","HEAD"],cwd=root,text=True).strip(),"checks":[],"release_complete":False}
def limits():
 resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(900,900));resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,512*1024**2))
for name,command in checks:
 print("start",name,flush=True);r=supervise(command,cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=900,merged=True);raw=r["stdout"]
 log=out/(name+".log.gz");log.write_bytes(gzip.compress(raw,mtime=0))
 row={"name":name,"command":command,**evidence(r,include_output=False),"log":log.name,"log_sha256":hashlib.sha256(raw).hexdigest(),"passed":r["exit"]==0 and r["reason"]=="exit"}
 report["checks"].append(row);(out/"verification.json").write_text(json.dumps(report,indent=2)+"\n");print("finish",name,row["passed"],flush=True)
 if not row["passed"]:break
sys.exit(int(not all(x["passed"] for x in report["checks"])))
