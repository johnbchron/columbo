# AGENTS.md — columbo

SSR streaming suspense library for Rust. Render placeholders, stream async results, swap in-place via injected JS.

Single crate: `crates/columbo`. Edition 2024, nightly toolchain.

## VCS

JJ. Use `jj`, not `git`.

## Dev environment

Nix flake with direnv. The shell will load automatically if direnv is enabled:

```sh
cd ~/github/columbo  # direnv loads nix shell automatically
```

Or manually:

```sh
nix develop
```

## Build & test

```sh
# Default (with maud feature)
cargo build
cargo test

# Without maud
cargo test --no-default-features

# Example (requires maud)
cargo run --example axum-example   # :3000
```

## Architecture

**Core types:**
- `Html` — Newtype wrapper around `String` for pre-escaped HTML. Central type for all markup.
- `HtmlStream` — Byte stream that chains the document with suspended results.
- `Suspense` — Placeholder handle returned by `suspend()`.

**Key trait:** Everything converts to `Html` via `Into<Html>`:
- `From<String>`, `From<&str>` (always)
- `From<maud::Markup>` (feature-gated behind `maud`)

**Features:**
- `default = ["maud"]` — maud integration enabled by default
- `maud` — Optional maud dependency; gates `From<maud::Markup>` and `impl maud::Render for Suspense`

## Source layout

| File | Role |
|---|---|
| `src/lib.rs` | Public API: `new`, `SuspenseContext`, `SuspendedResponse`, `ColumboOptions`, `Html` export |
| `src/html.rs` | `Html` newtype, `From` impls, `Display` |
| `src/format.rs` | HTML string generation (placeholders, replacements, panic fallback, script injection). No maud imports. |
| `src/html_stream.rs` | `HtmlStream` — chains document + receiver into a byte stream |
| `src/run_suspended.rs` | Runs suspended futures (generic over `M: Into<Html>`), catches panics, sends to channel |
| `src/cancel_on_drop.rs` | Cancels token on drop |
| `src/columbo.js` | Client-side swap (MutationObserver + View Transitions). `include_str!`'d — changes require rebuild |
| `src/tests/mod.rs` | Test module root, helper functions (`collect_stream`, `parse`, `select`) |
| `src/tests/string_based.rs` | String-based tests (no maud) — 5 tests |
| `src/tests/placeholders.rs` | Placeholder rendering tests — 2 tests |
| `src/tests/panic_handling.rs` | Panic handling tests — 2 tests |
| `src/tests/cancellation.rs` | Cancellation tests — 2 tests |
| `src/tests/edge_cases.rs` | Edge case tests — 1 test |
| `src/tests/maud_integration.rs` | Maud integration tests (`#[cfg(feature = "maud")]`) — 6 tests |

## Conventions

- **All HTML is `Html`**: No direct use of `maud::Markup` in library internals. External types convert via `Into<Html>`.
- **Internal modules are `pub(crate)`.**
- **Framework-agnostic**: Produces a `Stream<Item = Result<Bytes, io::Error>>`. Axum only in examples.
- **Tests in `src/tests/`**: Organized by concern, each module in its own file with doc comments.
- **Feature gates**: Use `#[cfg(feature = "maud")]` for maud-specific code.
- **Test both feature configurations**: Run `cargo test` (with maud) and `cargo test --no-default-features` (without maud) after changes.

## Testing

Tests use `scraper` to parse collected HTML output. Helper functions in `src/tests/mod.rs`:
- `collect_stream(stream)` — Collects all chunks into a string
- `parse(html)` — Parses HTML into scraper document
- `select(doc, css)` — CSS selector helper

**Test coverage:**
- 18 tests with `--all-features` (maud enabled)
- 12 tests with `--no-default-features` (string-only)

## Adding maud integration to other template engines

Implement `From<YourType> for Html`:

```rust
impl From<YourTemplate> for columbo::Html {
    fn from(t: YourTemplate) -> Self {
        Html::new(t.render())
    }
}
```
