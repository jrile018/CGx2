# Normalized grid quote repair

The starter is a 20-module quote graph for four energy charge groups and their
14 named charge components. Usage arrives normalized as a signed 64-bit count.
Each component must apply its published rate to that count exactly once; group
totals and the overall quote sum the components with checked arithmetic.
Repair the aggregation paths while retaining all public charge and total APIs.
The default quote for usage `10` must be `2810`. Zero usage must be zero, and
an unrepresentable intermediate must fail rather than wrap. Change only the
starter you are given. The private suite tests group behavior and boundaries.

Allowed tools: local compiler, test runner, editor, and standard language tools.
Allowed documentation: the supplied starter, CGx2 package/compiler
documentation for the CGx2 condition, or the Rust standard library
documentation for Rust. No network dependency or foreign runtime is needed.
