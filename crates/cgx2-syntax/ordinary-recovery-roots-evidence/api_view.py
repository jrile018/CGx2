#!/usr/bin/env python3
"""Make the API runner's original five-artifact view from frozen bytes."""
import json
from pathlib import Path
import sys
from common import digest

b, out = map(lambda p: Path(p).resolve(), sys.argv[1:])
manifest = json.loads((b.parent / 'frozen.json').read_text())
index = Path(manifest['source_directory']) / 'libcgx2_api_index.rlib'
index_sha = digest(index)
matches = [name for name, value in manifest['dependency_sha256'].items()
           if name.startswith('libcgx2_api_index-') and name.endswith('.rlib') and value == index_sha]
assert len(matches) == 1, 'canonical API library must match the preexisting frozen dependency'
out.mkdir(exist_ok=False)
artifacts = {}
for name in ('cgx2', 'cgx2-native', 'cgx2c', 'libcgx2_cli.rlib', 'libcgx2_api_index.rlib'):
    source = index if name == index.name else b / name
    (out / name).symlink_to(source)
    artifacts[name] = digest(source)
(out / 'manifest.json').write_text(json.dumps({'artifact_sha256': artifacts,
    'api_index_frozen_dependency': matches[0]}, indent=2) + '\n')
print(json.dumps({'artifact_count': len(artifacts), 'api_index_frozen_dependency': matches[0]}))
