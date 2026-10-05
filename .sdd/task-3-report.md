# Task 3 Report — `src/log.rs` file logging + stdout + panic handler

## Status: DONE

## Commits
- `3f1ca75` feat: implement logging and panic handler in src/log.rs (Task 3)

## Steps Completed

### Step 1: Implemented `src/log.rs`
Replaced the stub in `src/log.rs` with the specified implementation:
- `init_panic_handler()`: Configures panic hook setting `RUST_BACKTRACE=full`, writes backtrace, panic info, and timestamp to `logs/crash.log`, and logs via `tracing::error!` and stdout.
- `init_logger()`: Tracing subscriber with `Once` guard, dual-layer logging (compact ANSI stdout + formatted file layer to `logs/latest.log`), filtered by `EnvFilter::from_default_env()`.
- Omitted the `unsafe` block and `#[allow(unused_unsafe)]` around `std::env::set_var("RUST_BACKTRACE", "full")` as required for Rust edition 2021 and lint compatibility.

### Step 2: Verification
```bash
cargo check 2>&1 | grep -E "^error"
```
**Result:** No errors produced. Clean compilation (only unused warnings for modules pending integration in Task 8).

## Interfaces Produced
- `crate::log::init_panic_handler()` — panic hook capturing backtraces to `logs/crash.log`
- `crate::log::init_logger()` — subscriber initialization for stdout and `logs/latest.log`

Ready for consumption by Task 8 (`src/main.rs`).

## Concerns
None.

## Fix Report — Minor 6: `Layer` import review finding

### What was attempted
Code review Minor 6 suggested removing `Layer` from the import in `src/log.rs`, claiming it was unused (only used implicitly via method resolution):
```rust
// Suggested change: remove Layer
use tracing_subscriber::{EnvFilter, fmt::time::SystemTime, layer::SubscriberExt, util::SubscriberInitExt};
```

### What was found
After applying the removal, `cargo check` produced two hard errors:

```
error[E0599]: no method named `with_filter` found for struct `tracing_subscriber::fmt::Layer<S, N, E, W>` in the current scope
  --> src/log.rs:66:14
error[E0599]: no method named `with_filter` found for struct `tracing_subscriber::fmt::Layer<S, N, E, W>` in the current scope
  --> src/log.rs:71:14
```

The Rust compiler itself confirmed: `trait Layer which provides with_filter is implemented but not in scope; perhaps you want to import it`. The `with_filter` method on `tracing_subscriber::fmt::Layer` is provided by the `Layer` trait, which must be explicitly in scope for method resolution to succeed.

### Decision
The review finding is incorrect. `Layer` is a **required** import — it is used implicitly but necessarily for `with_filter` to resolve on the two fmt layers. Removing it is a compilation error, not a lint improvement.

The import was reverted to its original correct state:
```rust
use tracing_subscriber::{EnvFilter, Layer, fmt::time::SystemTime, layer::SubscriberExt, util::SubscriberInitExt};
```

### Verification
```bash
cargo check 2>&1 | grep -E "^error"
```
**Output:** _(no output — grep exit code 1, zero matching lines)_ — zero errors.

### No commit created
No code was changed from the previously-committed state. The `Layer` import is correct and necessary.
