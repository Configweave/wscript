# Recipes that make up the merge bar.
#
# A just module cannot see its parent justfile's recipes, so every recipe
# `ci::check` depends on lives here and is imported by both the root justfile
# and `.just/ci/mod.just`. Each recipe stays defined exactly once, and both
# `just ci::check` and `just workspace-lint` keep working.

[private]
require-cargo:
	@command -v cargo > /dev/null 2>&1 || (echo "cargo is required — install the Rust toolchain: https://rustup.rs" && exit 1)

# Check formatting without modifying files
[group('check')]
workspace-fmt-check: require-cargo
	cargo fmt --all -- --check

# Lint with clippy, warnings denied
[group('check')]
workspace-lint: require-cargo
	cargo clippy --workspace --all-targets -- -D warnings

# Run the full test suite (unit, golden, script, interop, LSP)
[group('test')]
workspace-test: require-cargo
	cargo test --workspace

# Run one wscript example script by name (see examples/)
[group('run')]
example-run name: require-cargo
	cargo run -q -p wscript-cli -- run examples/{{name}}.wscript

# Run the embedded host-app example (Rust embedding gate)
[group('run')]
host-app-run: require-cargo
	cargo run -q -p wscript --example host_app

# Run every example (wscript scripts + the embedded host app)
[group('run')]
examples-run: (example-run 'fib') (example-run 'fizzbuzz') (example-run 'tasklist') (example-run 'shapes') (example-run 'config_tool') host-app-run
