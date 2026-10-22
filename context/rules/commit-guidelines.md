## Commit Guidelines

- Concise, one-line messages (multi-line only when many changes)
- Group related files logically
- No emojis
- Use `git diff` to understand changes before committing
- **Never** include AI-agent signatures in your commits
    - Example: "Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
    - Never commit with attribution metadata

## CI: how your commits get checked (run these BEFORE you push)

`.github/workflows/ci.yml` runs `test` and `lint` on every push to `main` and every pull request. Reproduce the gate locally first.

### 1. Format with nightly

`rustfmt.toml` enables four unstable options: `imports_granularity = "Module"` (one `use` line per module), `group_imports = "StdExternalCrate"` (std, then external crates, then this crate, as separate blocks), `hex_literal_case = "Lower"`, and `format_macro_matchers = true`. Stable `cargo fmt` silently skips all of them: your code looks formatted locally but fails CI.

```bash
cargo +nightly fmt        # not `cargo fmt`; stable can't apply the unstable rules
```

### 2. Clippy, warnings are errors

```bash
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps   # rustdoc warnings fail CI's lint job the way clippy's do
```

Lint levels live in `Cargo.toml` `[lints.clippy]`. The panic family (`unwrap`, `expect`, `panic`, indexing, slicing) is denied outside tests; `clippy.toml` allows it inside them.

### 3. Tests

```bash
cargo test                # unit and integration tests, no env needed
```
