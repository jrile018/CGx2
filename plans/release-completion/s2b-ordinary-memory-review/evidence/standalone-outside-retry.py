from pathlib import Path
import tempfile,runpy
outside=Path('/home/john-riley/.codex/visualizations/2026/10/23/01a167e4-6669-7803-b925-af52f00fe0f7/ordinary-standalone-review')
outside.mkdir(exist_ok=True)
original=tempfile.mkdtemp
def redirected(*args,**kwargs):
    if kwargs.get('dir')=='/tmp' and kwargs.get('prefix')=='cgx2-ordinary-memory-standalone-':
        kwargs['dir']=str(outside)
    return original(*args,**kwargs)
tempfile.mkdtemp=redirected
runpy.run_path('/home/john-riley/.codex/worktrees/4836/lang/crates/cgx2-syntax/ordinary-memory-evidence/standalone.py',run_name='__main__')
