# wscript build & test automation — run bare `just` to list recipes.

# Verification gate — the merge bar (`just ci::check`)
mod ci '.just/ci'

# The gate's recipes live in shared.just so the `ci` module and this justfile
# can each import them; a module cannot see its parent's recipes.
import '.just/shared.just'

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

# -------------------------------------------------------------- release

# Cutting a release is a trailer on an empty commit, not a recipe — see the
# Releases section of README.md. Only the version stamp lives here, because only
# the version stamp is repo-specific enough to get wrong quietly: the internal
# crates pin each other by version as well as by path (`wscript-core = { path =
# "wscript-core", version = "0.2.0" }`), so replacing only the
# `[workspace.package]` version — the obvious edit, and the one the rest of the
# fleet's release workflows make — leaves those requirements behind. A patch
# bump still resolves under caret semantics, so it looks fine right up until the
# first minor bump fails with "failed to select a version for the requirement".
# Replace every copy, then have cargo re-resolve the workspace to prove the
# manifest is coherent rather than merely edited.

# Stamp VERSION through the workspace manifest — what CI does before a release build
[group('build')]
release-stamp version: require-cargo
	#!/usr/bin/env bash
	set -euo pipefail
	old=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
	if [ -z "$old" ]; then
		echo "release-stamp: no \`version = \"…\"\` in Cargo.toml — has the manifest moved?" >&2
		exit 1
	fi
	# Escaped, so the dots in a version match dots and not any character.
	old_re=${old//./\\.}
	sed -i "s/version = \"$old_re\"/version = \"{{version}}\"/g" Cargo.toml
	if grep -q "version = \"$old_re\"" Cargo.toml; then
		echo "release-stamp: $old still present after stamping — the substitution missed something" >&2
		exit 1
	fi
	cargo metadata --format-version 1 > /dev/null
	echo "stamped {{version}} (was $old)"

# ----------------------------------------------------------------- test

# Regenerate wscript-std/wscripti/std.wscripti after changing stdlib registrations
[group('test')]
wscripti-regen:
	WSCRIPT_REGEN_WSCRIPTI=1 cargo test -p wscript-cli --test wscripti_gen

# Regenerate committed diagnostic/parser snapshots, then review with `git diff`
[group('test')]
snap-regen:
	WSCRIPT_REGEN_SNAP=1 cargo test -p wscript-compiler

# Regenerate tests/fuel.snap (what each script costs to run), then review the diff
[group('test')]
fuel-regen:
	WSCRIPT_REGEN_FUEL=1 cargo test -p wscript-cli --test fuel_snapshot

# Test everything
[group('test')]
test: workspace-test

# ---------------------------------------------------------------- check

# Apply rustfmt to the workspace
[group('check')]
workspace-fmt:
	cargo fmt --all

# ------------------------------------------------------------------ run

# Start the wscript REPL
[group('run')]
repl-start:
	cargo run -q -p wscript-cli -- repl

# Run microbenchmarks vs Lua/Rhai/Rune (release profile)
[group('run')]
bench-run:
	cargo run --release -p wscript-benchmarks

# ------------------------------------------------------------------ docs

# The docs site + wscript wskill are authored in wdoc and rendered by the `wcl`
# CLI (install: https://wcl.dev). These recipes need `wcl` on PATH.

# Serve the docs site with live reload (/ = landing, /wskills/wscript/ = reference)
[group('docs')]
docs-serve *ARGS:
	wcl wdoc serve docs/main.wcl {{ARGS}}

# Build the docs site into docs/_site/ (gitignored)
[group('docs')]
docs-build *ARGS:
	wcl wdoc build docs/main.wcl --out docs/_site {{ARGS}}

# Build the wscript skill into .claude/skills/wscript/ (committed) from the wscript wskill
[group('docs')]
skill-build *ARGS:
	wcl wdoc skill docs/wskills/wscript/wdoc/skill/main.wcl --out .claude/skills/wscript {{ARGS}}

# Validate the wskill model and every projection template
[group('docs')]
wskill-check:
	wcl check docs/wskills/wscript/wskill.wcl
	wcl check docs/wskills/wscript/wdoc/book/main.wcl
	wcl check docs/wskills/wscript/wdoc/skill/main.wcl
	@[ ! -f docs/wskills/wscript/wdoc/presentation/main.wcl ] || wcl check docs/wskills/wscript/wdoc/presentation/main.wcl
	@[ ! -f docs/wskills/wscript/wdoc/training/main.wcl ] || wcl check docs/wskills/wscript/wdoc/training/main.wcl
