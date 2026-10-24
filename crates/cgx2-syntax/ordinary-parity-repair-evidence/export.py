#!/usr/bin/env python3
"""Compress immutable attempt reports and exact inputs; never discard failed evidence."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
from common import HERE


def fingerprint(path):
    sha=hashlib.sha256();size=0
    with path.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1<<20),b''):
            sha.update(chunk);size+=len(chunk)
    return {'bytes':size,'sha256':sha.hexdigest()}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--scratch-dir',type=Path,required=True)
    p.add_argument('--output-dir',type=Path,required=True)
    a=p.parse_args();scratch=a.scratch_dir.resolve();out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=False)
    files={};summaries={};exported=set()
    def export(source,relative):
        relative=Path(relative)
        key=str(relative)+'.gz'
        if key in exported:return
        target=out/key;target.parent.mkdir(parents=True,exist_ok=True)
        with source.open('rb') as incoming,target.open('wb') as raw:
            with gzip.GzipFile(filename='',mode='wb',fileobj=raw,mtime=0) as zipped:
                shutil.copyfileobj(incoming,zipped)
        files[key]={'original_path':str(source),**fingerprint(source),'gzip':fingerprint(target)}
        exported.add(key)
    for attempt in sorted(scratch.iterdir()):
        if not attempt.is_dir() or not (attempt/'report.json').exists():continue
        report=json.loads((attempt/'report.json').read_text())
        summaries[attempt.name]={k:report.get(k) for k in ('passed','failed','complete','exceptions')}
        for source in attempt.rglob('*'):
            if source.is_file() and source.suffix in ('.json','.cg','.toml','.py','.rs','.txt','.log') and 'frozen' not in source.relative_to(attempt).parts:
                export(source,Path('attempts')/source.relative_to(scratch))
        for tag,artifact in report.get('standalone_artifacts',{}).items():
            export(Path(artifact['source']),Path('standalone-source')/attempt.name/(tag+'.rs'))
        if not report.get('standalone_artifacts'):
            for command in report.get('commands',[]):
                argv=command['argv']
                if 'emit-rust' in argv and '--out' in argv:
                    source=Path(argv[argv.index('--out')+1])
                    if source.suffix=='.rs' and source.is_file():
                        export(source,Path('standalone-source')/attempt.name/(command['name']+'.rs'))
    (out/'manifest.json').write_text(json.dumps({'schema_version':1,'files':files},indent=2)+'\n')
    (out/'summary.json').write_text(json.dumps({'schema_version':1,'reviewed_base':'89464a76c9bd84a3b090ec58a14ca6be0043c6ea',
        'attempts':summaries,'ordinary_memory_accepted':False,'release_accepted':False,
        'interpretation':'Semantic failures remain failed. Timing fields supervise bounded commands only. Review report distinguishes source/library/unrun gates.'},indent=2)+'\n')
    print(json.dumps({'exported_files':len(files),'attempts':summaries}))


if __name__=='__main__':main()
