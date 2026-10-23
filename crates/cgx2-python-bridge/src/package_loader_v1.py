"""Trusted embedded loader. Hash/signature preflight happens in Rust before entry.

This controls package resolution in a trusted single-threaded embedder. It is not
an OS sandbox and does not restrain arbitrary effects by approved package code.
"""
import sys
from importlib.machinery import ModuleSpec
from importlib import import_module
from importlib.machinery import BuiltinImporter, FrozenImporter, PathFinder, SourceFileLoader
import os
import builtins
import importlib


def invoke(sources, stdlib_paths, module, attribute, args, kwargs):
    roots = {name.split('.')[0] for name in sources}
    if roots.intersection(sys.stdlib_module_names) or roots.intersection(sys.builtin_module_names):
        raise ImportError('pinned package cannot shadow the trusted standard library')

    def owned(name):
        return name.split('.')[0] in roots

    # A cached module could bypass our loader entirely. Never reuse it.
    if any(owned(name) for name in sys.modules):
        raise ImportError('pinned package namespace already loaded')

    def check_import(name):
        root = name.split('.')[0]
        if root not in roots and root not in sys.stdlib_module_names and root not in sys.builtin_module_names:
            raise ModuleNotFoundError('dependency absent from package lock: ' + name)

    original_import = builtins.__import__
    original_import_module = importlib.import_module

    def checked_import(name, globals=None, locals=None, fromlist=(), level=0):
        if not level:
            check_import(name)
        return original_import(name, globals, locals, fromlist, level)

    def checked_import_module(name, package=None):
        if not name.startswith('.'):
            check_import(name)
        return original_import_module(name, package)

    class SnapshotLoader:
        def create_module(self, spec):
            return None

        def exec_module(self, target):
            source, filename, is_package = sources[target.__name__]
            target.__file__ = filename
            target.__dict__['__builtins__'] = dict(vars(builtins), __import__=checked_import)
            if is_package:
                target.__path__ = []
            exec(compile(source, filename, 'exec', dont_inherit=True), target.__dict__)

    class NoBytecodeLoader(SourceFileLoader):
        def get_code(self, fullname):
            # Ignored __pycache__ files must never override a verified .py file.
            return compile(self.get_data(self.path), self.path, 'exec', dont_inherit=True)

    class SnapshotFinder:
        def find_spec(self, fullname, path=None, target=None):
            if fullname in sources:
                source, filename, is_package = sources[fullname]
                return ModuleSpec(fullname, SnapshotLoader(), origin=filename,
                                  is_package=is_package)
            if owned(fullname):
                # No disk fallback: package __path__ is empty. Pinned Python code
                # may register a virtual-module finder (notably six.moves).
                return None
            if fullname.split('.')[0] not in sys.stdlib_module_names:
                raise ModuleNotFoundError('dependency absent from package lock: ' + fullname)
            spec = PathFinder.find_spec(fullname, path)
            if spec is None:
                return None
            origin = os.path.realpath(spec.origin or '')
            if (not any(origin.startswith(root + os.sep) for root in stdlib_paths)
                    or any(part in {'__pycache__', 'site-packages', 'dist-packages'}
                           for part in origin.split(os.sep))):
                raise ImportError('stdlib origin outside pinned directories: ' + fullname)
            if isinstance(spec.loader, SourceFileLoader):
                spec.loader = NoBytecodeLoader(fullname, origin)
            elif not origin.endswith(tuple(__import__('importlib').machinery.EXTENSION_SUFFIXES)):
                raise ImportError('unsupported stdlib loader: ' + fullname)
            return spec

    finder = SnapshotFinder()
    old_path, old_meta = sys.path, sys.meta_path
    old_bytecode = sys.dont_write_bytecode
    try:
        # Ignore site packages, cwd, .pth files and custom host meta-path finders.
        # Only the trusted built-in/frozen/stdlib finders follow our snapshot finder.
        sys.meta_path = [BuiltinImporter, FrozenImporter, finder]
        sys.path = list(stdlib_paths)
        sys.dont_write_bytecode = True
        importlib.import_module = checked_import_module
        target = import_module(module)
        target = getattr(target, attribute)
        if not callable(target):
            raise TypeError('declared target is not callable')
        return target(*args, **kwargs)
    finally:
        sys.path, sys.meta_path = old_path, old_meta
        sys.dont_write_bytecode = old_bytecode
        importlib.import_module = original_import_module
        for name in tuple(sys.modules):
            if owned(name):
                del sys.modules[name]
        # No package state or Python handle is retained by this route.
