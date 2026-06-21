# wscript build & test automation — run bare `just` to list recipes.

[default, private]
main:
	@just --list

# ---------------------------------------------------------------- build

# Build every workspace crate (debug)
[group('build')]
workspace-build:
	cargo build --workspace

# Build every workspace crate (release)
[group('build')]
workspace-build-release:
	cargo build --workspace --release

# Build everything
[group('build')]
build: workspace-build

# ----------------------------------------------------------------- test

# Run the full test suite (unit, golden, script, interop, LSP)
[group('test')]
workspace-test:
	cargo test --workspace

# Regenerate wscript-std/wscripti/std.wscripti after changing stdlib registrations
[group('test')]
wscripti-regen:
	WSCRIPT_REGEN_WSCRIPTI=1 cargo test -p wscript-cli --test wscripti_gen

# Test everything
[group('test')]
test: workspace-test

# ---------------------------------------------------------------- check

# Check formatting without modifying files
[group('check')]
workspace-fmt-check:
	cargo fmt --all -- --check

# Apply rustfmt to the workspace
[group('check')]
workspace-fmt:
	cargo fmt --all

# Lint with clippy, warnings denied
[group('check')]
workspace-lint:
	cargo clippy --workspace --all-targets -- -D warnings

# Full gate: format check, lint, tests, examples
[group('check')]
check: workspace-fmt-check workspace-lint workspace-test examples-run

# ------------------------------------------------------------------ run

# Run one wscript example script by name (see examples/)
[group('run')]
example-run name:
	cargo run -q -p wscript-cli -- run examples/{{name}}.wscript

# Run every example (wscript scripts + the embedded host app)
[group('run')]
examples-run: (example-run 'fib') (example-run 'fizzbuzz') (example-run 'tasklist') (example-run 'shapes') (example-run 'config_tool') host-app-run

# Run the embedded host-app example (Rust embedding gate)
[group('run')]
host-app-run:
	cargo run -q -p wscript --example host_app

# Start the wscript REPL
[group('run')]
repl-start:
	cargo run -q -p wscript-cli -- repl

# Run microbenchmarks vs Lua/Rhai/Rune (release profile)
[group('run')]
bench-run:
	cargo run --release -p wscript-benchmarks
