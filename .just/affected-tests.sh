#!/usr/bin/env bash
# Run the tests that the changed files can affect.
#
# TERRARIUM_CHANGED_FILES lists the changed paths (newline or space separated),
# relative to the repo root. Unset or empty: the full suite (every test and every
# example), the same as `just ci::check` without the format and lint steps.
#
# A file in a crate selects that crate and every workspace crate that depends on
# it (normal, dev or build edge). tests/ belongs to wscript-cli. examples/ runs
# the examples. docs/, .claude/ and Markdown files select nothing. Any other path
# (Cargo.toml, Cargo.lock, justfile, .just/, .github/, ...) selects the full suite.
set -euo pipefail

full() {
	echo "affected-tests: full suite ($1)"
	cargo test --workspace
	just examples-run
	exit 0
}

changed=$(printf '%s\n' "${TERRARIUM_CHANGED_FILES:-}" | tr ' ' '\n' | sed '/^$/d')
[ -n "$changed" ] || full "TERRARIUM_CHANGED_FILES is empty"

pkgs=()
examples=0
while IFS= read -r f; do
	case "$f" in
		wscript/*) pkgs+=(wscript) ;;
		wscript-core/*) pkgs+=(wscript-core) ;;
		wscript-compiler/*) pkgs+=(wscript-compiler) ;;
		wscript-vm/*) pkgs+=(wscript-vm) ;;
		wscript-std/*) pkgs+=(wscript-std) ;;
		wscript-macros/*) pkgs+=(wscript-macros) ;;
		wscript-cli/* | tests/*) pkgs+=(wscript-cli) ;;
		benchmarks/*) pkgs+=(wscript-benchmarks) ;;
		examples/*) examples=1 ;;
		docs/* | .claude/* | *.md) ;;
		*) full "$f is outside every crate" ;;
	esac
done <<< "$changed"

if [ "${#pkgs[@]}" -eq 0 ] && [ "$examples" -eq 0 ]; then
	echo "affected-tests: no code changed, no tests to run"
	exit 0
fi

members=" wscript wscript-core wscript-compiler wscript-vm wscript-std wscript-macros wscript-cli wscript-benchmarks "
selected=$(
	for p in $(printf '%s\n' "${pkgs[@]}" | sort -u); do
		cargo tree --workspace --offline -i "$p" -e normal,dev,build --prefix none --format '{p}' 2> /dev/null \
			|| cargo tree --workspace -i "$p" -e normal,dev,build --prefix none --format '{p}'
	done | awk '{print $1}' | while read -r n; do
		case "$members" in *" $n "*) echo "$n" ;; esac
	done | sort -u
)

if [ -n "$selected" ]; then
	args=()
	for p in $selected; do args+=(-p "$p"); done
	echo "affected-tests: cargo test ${args[*]}"
	cargo test "${args[@]}"
fi

# Every example runs through the wscript crate, so a change to any crate except
# the benchmarks can change what an example does.
if [ "$examples" -eq 1 ] || { [ -n "$selected" ] && printf '%s\n' "$selected" | grep -qv '^wscript-benchmarks$'; }; then
	just examples-run
fi
