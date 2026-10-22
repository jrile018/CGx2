# CGx2 execution policy prototype

This standalone crate selects sequential or scoped-thread CPU execution for one
reviewed operation: `i64` elementwise `value * multiplier + addend`, with checked
arithmetic. Parallel chunks join in source order, so the first overflow has the
same index as sequential execution. `execute_map` constructs its own pure
`Workload` from the input slice and returns the decision and values.

```rust
use cgx2_execution_policy::{Capabilities, Policy, execute_map};

let input = vec![1_i64; 4096];
let run = execute_map(&input, 2, 3, Policy::Auto, &Capabilities::default())?;
assert_eq!(run.values, vec![5; 4096]);
// Auto selects SequentialCpu until suitable measurements are supplied.
# Ok::<(), cgx2_execution_policy::ExecuteError>(())
```

`Policy::Auto` is the default. `Capabilities::default()` has
`parallel_workers = 1`, no measurements, and no GPU backend. `SequentialCpu`
always runs the reference path. `ParallelCpu` requires `parallel_workers >= 2`,
at least 2,048 elements, and a pure, independent checked-map workload with no
observable order, aliases, CGx2 memory activity, effects, or `no_gc` guard.
Explicit unsupported or unsafe requests return `PlanError`; `Auto` uses the
sequential path. Direct `map_i64_checked_parallel` bounds workers to 32 and
chunks to at least 1,024 elements, and runs sequentially for small inputs.

`Measurements` names a worker count, input-size range, sample count, and expiry.
For `Auto`, it needs at least three samples, a future `valid_until`, and the
same worker count as the plan. Each `CostProfile` gives nanosecond intervals for
launch/setup, CPU work per element, host/device transfer, synchronization, and
expected memory pressure. The planner selects parallel only when its estimated
upper end-to-end cost is strictly below the sequential lower bound. It reports
`CostStatus::Calibrated` or `Uncalibrated` and a stable `ReasonCode`. These
intervals are supplied by a reviewer; a core count or one noisy timing cannot
establish benefit. No global optimum or native-language speed claim follows.

Raw calibration is opt-in:

```sh
cargo run -p cgx2-execution-policy --bin calibrate --release --offline -- \
  --sizes 2048,4096,65536 --workers 2,4 --samples 7 --warmups 2 \
  > execution-policy-samples.csv
```

The CSV records every scalar, parallel, and empty scoped-thread overhead sample
with size, requested workers, OS, architecture, available threads, and Rust
version. Validate results on the target machine, across relevant input sizes,
then conservatively derive interval profiles and expiry. The helper does not
turn noisy raw samples into an automatic policy or benchmark claim. On machines
with fewer available threads than requested workers, the result measures
oversubscription, not parallel capacity.

GPU execution is unsupported. `GpuCapability` cannot register an executable
backend; the unit test uses a **mock cost profile** to verify that even a free
GPU estimate cannot select GPU. A future typed-IR integration must prove
independence, effects, aliasing, ownership, managed-session and `no_gc` state
before constructing `Workload`. A future GPU backend also needs an executable
operation/device probe and validated numeric, transfer, synchronization, and
foreign-memory behavior. Foreign PyTorch device allocation is outside CGx2
region budgets. Float reductions have no contract here.
