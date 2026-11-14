#!/usr/bin/env python3
"""Optional network setup: pinned tools only under /tmp/cgx2-arch-review-proof.
Run explicitly with --allow-network. This is separate from offline verify.py.
"""
import argparse, hashlib, json, os, pathlib, re, shutil, subprocess, tarfile, urllib.request
HERE=pathlib.Path(__file__).resolve().parent
P=pathlib.Path('/tmp/cgx2-arch-review-proof')

def main():
 a=argparse.ArgumentParser();a.add_argument('--allow-network',action='store_true');args=a.parse_args()
 if not args.allow_network:a.error('setup requires explicit --allow-network; verify.py is offline')
 pins=json.loads((HERE/'pins.json').read_text());P.mkdir(exist_ok=True)
 env=os.environ.copy();env.update(RUSTUP_HOME=str(P/'rustup'),CARGO_HOME=str(P/'cargo'),LEAN_NUM_THREADS='2',CARGO_BUILD_JOBS='2',MATHLIB_CACHE_DIR=str(P/'mathlib-cache'))
 def run(cmd,cwd=P):
  print('$',*cmd,flush=True);subprocess.run(cmd,cwd=cwd,env=env,check=True)
 def fetch(url,name):
  dest=P/name
  if not dest.exists():urllib.request.urlretrieve(url,dest)
  expected=pins['archives'][name]['sha256']
  with dest.open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
  if actual!=expected:raise RuntimeError('archive SHA256 mismatch: '+name)
  return dest
 aeneas=fetch('https://github.com/AeneasVerif/aeneas/releases/download/'+pins['aeneas_release']+'/aeneas-linux-x86_64.tar.gz','aeneas-linux-x86_64.tar.gz')
 with tarfile.open(aeneas) as t:t.extractall(P,filter='data')
 aeneas.unlink()
 lean=fetch('https://github.com/leanprover/lean4/releases/download/v4.36.0-rc2/lean-4.36.0-rc2-linux.tar.zst','lean-4.36.0-rc2-linux.tar.zst')
 (P/'lean').mkdir(exist_ok=True)
 run(['tar','--zstd','-xf',str(lean),'-C',str(P/'lean'),'--strip-components=1','--exclude=*.ilean','--exclude=*/src/*','--exclude=*/share/*'])
 lean.unlink()
 compiled=fetch('https://github.com/AeneasVerif/aeneas/releases/download/'+pins['aeneas_release']+'/lean-build-aeneas-x86_64-unknown-linux-gnu.tar.gz','lean-build-aeneas-x86_64-unknown-linux-gnu.tar.gz')
 lib=P/'backends/lean';build=lib/'.lake/build';build.mkdir(parents=True,exist_ok=True)
 with tarfile.open(compiled) as t:t.extractall(build,filter='data')
 compiled.unlink()
 # Install a copy of rustup into this isolated Cargo home so rustup's final
 # proxy/self check does not target or mutate the user's shared installation.
 rustup=shutil.which('rustup')
 if rustup is None:raise RuntimeError('rustup bootstrap prerequisite absent; install an isolated official rustup binary here first')
 bindir=P/'cargo/bin';bindir.mkdir(parents=True,exist_ok=True)
 if pathlib.Path(rustup).resolve()!= (bindir/'rustup').resolve():shutil.copy2(rustup,bindir/'rustup')
 env['PATH']=str(P/'lean/bin')+':'+str(bindir)+':'+env['PATH']
 run([str(bindir/'rustup'),'toolchain','install',pins['rust_toolchain'],'--profile','minimal','--component','rustc-dev','--component','rust-src'])
 for dep in json.loads((lib/'lake-manifest.json').read_text())['packages']:
  dest=lib/'.lake/packages'/dep['name'];dest.mkdir(parents=True,exist_ok=True)
  if not (dest/'.git').exists():
   run(['git','init','-q'],dest);run(['git','remote','add','origin',dep['url']],dest)
  run(['git','fetch','--depth=1','origin',dep['rev']],dest);run(['git','reset','--hard',dep['rev']],dest)
 run([str(P/'lean/bin/lake'),'build','@mathlib/cache'],lib)
 roots=(HERE/'source/external-roots.txt').read_text().splitlines()
 run([str(P/'lean/bin/lake'),'env','.lake/packages/mathlib/.lake/build/bin/cache','get',*roots],lib)
 print('Setup complete. Run offline verify.py; no source proof is claimed by setup alone.')
if __name__=='__main__':main()
