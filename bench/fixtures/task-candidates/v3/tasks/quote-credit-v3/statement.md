# Quote credit repair

The starter computes a fixed-width signed 64-bit quote from units, unit price,
shipping, and a customer credit. A credit reduces the invoice once, after
shipping is added. Repair the existing implementation while keeping the public
four-argument quote API and checked arithmetic behavior. The default example
must produce `824` for `(7, 120, 18, 34)`. Zero and boundary inputs must keep
their mathematically specified behavior; an unrepresentable intermediate must
fail instead of wrapping. Change only the starter you are given. The private
acceptance suite tests other inputs and the boundary case.

Allowed tools: local compiler, test runner, editor, and standard language tools.
Allowed documentation: the supplied starter, CGx2 language/CLI documentation
for the CGx2 condition, or the Rust standard library documentation for Rust.
No network dependency or foreign runtime is needed.
