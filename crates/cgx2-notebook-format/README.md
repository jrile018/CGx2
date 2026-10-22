# CGx2 notebook format

This standalone crate reads and writes version 1 `.cgnb` documents. It does not compile or execute CGx2 code. Saved outputs are cached observations with their execution ordinals, not evidence that a cell is current or correct.

## Text grammar

The document starts with `#!cgnb 1` and a newline. Headers start in column zero and end with a newline:

```text
#!cgnb 1
%% markdown id=notes
# Notebook notes
%% code id=load
let value = 2
%% output id=load mime=text/plain ordinal=1
2
%% asset id=load mime=image/png ordinal=1 sha256=<64 lowercase hex characters>
%% provenance id=load schema=2 source_sha256=<64 lowercase hex characters> prefix_sha256=<64 lowercase hex characters> evaluator=cgx2-pure-scalar-v1 ordinal=1 output_ordinal=1 result=success
```

Cell IDs contain 1–64 ASCII letters, digits, `_`, or `-`, and must be unique. Output `id` refers to an earlier code cell. MIME values are ASCII tokens with a slash (for example `text/plain` or `image/png`); ordinals are unsigned 64-bit decimal integers. Writers group each cell's outputs immediately after its source and keep cell/output order. A Markdown cell cannot have outputs.

Cell sources and text output bodies continue until the next column-zero header. A body line whose content begins `%%` is prefixed with a backslash. A line with one or more leading backslashes followed by `%%` receives one additional backslash, so the original bytes round-trip. Other body bytes, including blank lines and trailing newline state, are preserved. If a body does not end in a newline, the writer adds one separator newline followed by `%% end`; the reader removes that separator. An empty body needs no `%% end`.

Headers use LF. Body text is UTF-8 and may contain CRLF. `parse_notebook_bytes` reports invalid UTF-8; `parse_notebook` accepts a Rust `&str`, which is already valid UTF-8. The complete document, including embedded data, is limited to 16 MiB. The parser rejects unsupported versions, duplicate/invalid IDs, unknown output references, malformed headers, and invalid asset digests with typed errors.

## Execution provenance

A code cell may have one provenance header after its outputs. Schema 1 and 2 are supported with evaluator cgx2-pure-scalar-v1. Success records completed evaluation; failed records an attempted evaluation while retained output is an older observation. The execution ordinal counts attempts and continues from saved provenance after reopen. The output ordinal is the display output number or none for silent cells. Parser and writer reject duplicate records, invalid digests, unsupported fields, and mismatched output ordinals.

source_sha256 is SHA-256 of exact unescaped UTF-8 source bytes. In schema 1, prefix_sha256 hashes every earlier code cell ID and source in file order. In schema 2, the same serialized field holds a dependency digest over direct producers and their transitive source dependencies. A matching record starts cached after reopen; a changed source or dependency starts stale. Neither state restores live bindings.

## Captured text records

Kernel-produced display is one UTF-8 output body with MIME exactly text/plain, a stable cell ID, and an output ordinal. Decoded text is limited to **65,536 UTF-8 bytes per output**; parsing and writing reject larger bodies or another text MIME. The associated schema=2 provenance is the versioned record of execution revision (ordinal), source digest (source_sha256), and dependency digest (prefix_sha256, retained as the serialized field name for compatibility). The digest covers direct producer cells and their transitive source dependencies. Unrelated earlier edits leave it unchanged. Schema 1 remains readable and uses the earlier whole-prefix digest. Importing either schema yields a cached or stale observation, never live execution state. Text survives ordinary save and self-contained export/import without executing cells. External files, network effects, HTML execution, and arbitrary commands cannot be produced by the pure scalar kernel.

## Assets and portable export

Pass a sidecar directory such as `example.cgnb.assets` as `asset_root` to `save_asset` and `load_asset`. Each filename is the lowercase SHA-256 digest of its bytes. A digest is validated before it becomes a path component; loads verify both file size and hash. An asset is limited to 8 MiB. A notebook is limited to 32 distinct assets totaling 12 MiB, and the complete UTF-8 file is limited to 16 MiB. Saving identical bytes reuses the same filename. Existing corrupt files, symlink assets, and symlinks in the asset directory path are rejected.

`export_self_contained` appends blocks after the notebook content, sorted by digest:

```text
%% embedded sha256=<64 lowercase hex characters> mime=image/png size=<decoded byte count>
<one base64 line>
```

The only portable asset MIME values are `image/png`, `image/jpeg`, `image/gif`, and `image/webp`. The embedded MIME must exactly match every output reference to that digest. Export and import reject SVG, HTML, scripts, unknown MIME, conflicting MIME references, duplicate or unreferenced blocks, traversal-like digests, hash or length mismatch, and oversized decoded or encoded data. There is no compression or external URL lookup. `parse_self_contained` validates every block in memory before exposing a `NotebookDocument`; `open_notebook_bytes` also opens ordinary version 1 files using verified sidecars. `import_self_contained` verifies before restoring files into the chosen sidecar directory. The caller chooses that directory; no path is read from notebook text.

Portable export retains the `#!cgnb 1` header and readable native cells, then adds an embedded block section. It is an extension of version 1, not a new execution format. Older readers that only call `parse_notebook` reject the embedded headers and must import to ordinary version 1 plus sidecars. A portable file with no assets is identical to ordinary version 1. Saved provenance remains cached or stale after reopen; importing bytes never creates live evaluator state.

## Scope

There is no live kernel, cell rerun, stale-dependency tracking, output rendering, HTML sanitization, or `.ipynb` conversion in this crate. The future notebook interface must provide those behaviors and use the same CGx2 parser/checker/runtime as the CLI.

The provenance `state=stale` suffix records a restart without changing a cell's stable ID, source/dependency digests, evaluator, or execution/output ordinals. It is saved cache status, not evidence that the cell ran again. A later successful execution writes `result=success` with a new revision.
