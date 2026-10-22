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
```

Cell IDs contain 1–64 ASCII letters, digits, `_`, or `-`, and must be unique. Output `id` refers to an earlier code cell. MIME values are ASCII tokens with a slash (for example `text/plain` or `image/png`); ordinals are unsigned 64-bit decimal integers. Writers group each cell's outputs immediately after its source and keep cell/output order. A Markdown cell cannot have outputs.

Cell sources and text output bodies continue until the next column-zero header. A body line whose content begins `%%` is prefixed with a backslash. A line with one or more leading backslashes followed by `%%` receives one additional backslash, so the original bytes round-trip. Other body bytes, including blank lines and trailing newline state, are preserved. If a body does not end in a newline, the writer adds one separator newline followed by `%% end`; the reader removes that separator. An empty body needs no `%% end`.

Headers use LF. Body text is UTF-8 and may contain CRLF. `parse_notebook_bytes` reports invalid UTF-8; `parse_notebook` accepts a Rust `&str`, which is already valid UTF-8. The complete document, including embedded data, is limited to 16 MiB. The parser rejects unsupported versions, duplicate/invalid IDs, unknown output references, malformed headers, and invalid asset digests with typed errors.

## Assets and portable export

Pass a sidecar directory such as `example.cgnb.assets` as `asset_root` to `save_asset` and `load_asset`. Each filename is the lowercase SHA-256 digest of its bytes. A digest is validated before it becomes a path component; loads verify both file size and hash. An asset is limited to 8 MiB. Saving identical bytes reuses the same filename. Existing corrupt files and symlinks are rejected.

`export_self_contained` appends blocks after the notebook content, sorted by digest:

```text
%% embedded sha256=<64 lowercase hex characters> size=<decoded byte count>
<one base64 line>
```

`import_self_contained` verifies every embedded digest and size, requires one block for every referenced asset, then restores files into the chosen sidecar directory. It works when the original sidecar is absent. Use `import_self_contained` for exported documents; `parse_notebook` intentionally rejects embedded blocks. The caller chooses the sidecar directory; these functions do not derive paths from notebook names.

## Scope

There is no live kernel, cell rerun, stale-dependency tracking, output rendering, HTML sanitization, or `.ipynb` conversion in this crate. The future notebook interface must provide those behaviors and use the same CGx2 parser/checker/runtime as the CLI.
