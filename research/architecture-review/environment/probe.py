#!/usr/bin/env python3
"""Offline readiness replay. Uses only lane sources + installed isolated runtimes.
No downloads, package installs, shared environment writes or performance timings.
"""
import argparse, hashlib, json, os, pathlib, shlex, shutil, subprocess, sys
P=pathlib.Path(__file__).resolve().parent
ap=argparse.ArgumentParser()
ap.add_argument('--output',type=pathlib.Path,default=pathlib.Path('/tmp/cgx2-arch-review-environment/replay'))
ap.add_argument('--python',default='/tmp/cgx2-arch-review-environment/venv/bin/python')
ap.add_argument('--ocamlopt',default='/home/john-riley/.opam/serena-ocaml/bin/ocamlopt')
args=ap.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
build=out/'build';build.mkdir(exist_ok=True)
env=dict(os.environ,OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',CARGO_BUILD_JOBS='2',PYTHONDONTWRITEBYTECODE='1')
checks=[]
def run(id,cmd,scope,timeout=120,unavailable_is_blocker=False):
    try:
        p=subprocess.run([str(x) for x in cmd],cwd=build,env=env,capture_output=True,text=True,timeout=timeout)
        code=p.returncode;stdout=p.stdout;stderr=p.stderr
    except (OSError,subprocess.TimeoutExpired) as e:
        code=127;stdout='';stderr=str(e)
    evidence=f'{id}.log'
    (out/evidence).write_text('$ '+shlex.join([str(x) for x in cmd])+'\nexit_code='+str(code)+'\nSTDOUT\n'+stdout+'\nSTDERR\n'+stderr)
    checks.append({'id':id,'status':'pass' if code==0 else ('blocked' if unavailable_is_blocker else 'fail'),'command':shlex.join([str(x) for x in cmd]),'exit_code':code,'evidence_file':evidence,'scope':scope})
    print(id,code,flush=True)
    return code,stdout
run('inventory',[sys.executable,P/'inventory.py'],'actual host devices, permissions, runtimes, versions')
for f in ['fixture.rs','fixture.ml']:
    shutil.copy2(P/f,build/f)
if run('rust_compile',['rustc','--edition=2021',build/'fixture.rs','-o',build/'rust-fixture'],'standalone Rust CPU compile')[0]==0:
    run('rust_cpu',[build/'rust-fixture'],'Rust buffer/failure fixture')
if run('ocaml_compile',[args.ocamlopt,'-o',build/'ocaml-fixture',build/'fixture.ml'],'OCaml native CPU compile')[0]==0:
    run('ocaml_cpu',[build/'ocaml-fixture'],'OCaml alias/data/failure fixture')
run('python_cpu',[args.python,P/'fixture.py'],'coherent isolated Python CPU fixture')
run('torch_cpu',[args.python,P/'torch_probe.py'],'Python Torch CPU alias/failure/inference fixture')
# uv pip freeze reads environment without a pip installation or network.
run('python_pins',['uv','pip','freeze','--python',args.python],'installed isolated Python package pins')
code,torchdir=run('torch_location',[args.python,'-c','import pathlib,torch; print(pathlib.Path(torch.__file__).parent)'],'native LibTorch wheel root')
if code==0:
    td=pathlib.Path(torchdir.strip());lib=td/'lib'
    cmd=['c++','-std=c++17','-O0','-D_GLIBCXX_USE_CXX11_ABI=1','-I'+str(td/'include'),'-I'+str(td/'include/torch/csrc/api/include'),P/'native.cpp','-L'+str(lib),'-Wl,-rpath,'+str(lib),'-ltorch_cpu','-ltorch','-lc10','-o',build/'native-libtorch']
    if run('native_compile',cmd,'native ATen CPU link using same isolated Torch2.11.0 closure')[0]==0:
        run('native_cpu',[build/'native-libtorch'],'native LibTorch CPU fixture and mapped-libpython exclusion')
        run('native_ldd',['ldd',build/'native-libtorch'],'resolved transitive ELF dependencies; inspect not-found and libpython')
    manifest=[]
    for f in sorted(lib.glob('*.so*')):
        manifest.append({'path':str(f),'bytes':f.stat().st_size,'sha256':hashlib.sha256(f.read_bytes()).hexdigest()})
    (out/'libtorch-libraries.json').write_text(json.dumps(manifest,indent=2)+'\n')
run('torch_gpu_attempt',[args.python,'-c',"import torch; print('torch=',torch.__version__,'hip=',torch.version.hip,'cuda=',torch.version.cuda,flush=True); print(torch.ones(4,device='cuda'))"],'attempt PyTorch GPU allocation; CPU fallback forbidden',unavailable_is_blocker=True)
run('gpu_vulkan',[args.python,P/'wgpu_probe.py'],'real selected AMD RX9070XT Vulkan i32/f32 compute, synchronization/readback')
(out/'checks.json').write_text(json.dumps(checks,indent=2)+'\n')
print('Evidence:',out)
sys.exit(0 if all(c['status'] in ['pass','blocked'] for c in checks) else 1)
