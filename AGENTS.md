# AGENTS.md — columbo

SSR streaming suspense library for Rust. Render placeholders, stream async results, swap in-place via injected JS.

Single crate: `crates/columbo`. Edition 2024, nightly toolchain.

## VCS

JJ. Use `jj`, not `git`.

## Build & test

```sh
cargo build
cargo test
cargo run --example axum-example   # :3000
```

## Source layout

| File | Role |
|---|---|
| `src/lib.rs` | Public API (`new`, `SuspenseContext`, `SuspendedResponse`, `ColumboOptions`) |
| `src/format.rs` | HTML rendering (placeholders, replacements, panic fallback, script injection) |
| `src/markup_stream.rs` | `MarkupStream` — chains document + receiver into a byte stream |
| `src/run_suspended.rs` | Runs suspended futures, catches panics, sends to channel |
| `src/cancel_on_drop.rs` | Cancels token on drop |
| `src/columbo.js` | Client-side swap (MutationObserver + View Transitions). `include_str!`'d — changes require rebuild |

## Conventions

- All HTML via maud `Render` trait.
- Internal modules are `pub(crate)`.
- Library is framework-agnostic (produces a `Stream`); axum only in examples/tests.
- Tests use `scraper` to parse collected HTML output.
- Run `cargo test` after any change.
