# just runs a module's recipes from the module's own directory, which would
# break every repo-root-relative path in the gate (`examples/*.wscript`). Point
# them back at the repo root. The path is relative to this file, and must be a
# literal — `set` cannot call `justfile_directory()`.
set working-directory := '../..'

import '../shared.just'

[default, private]
main:
	@just --list ci

# Everything a change must pass before it can merge
[group('ci')]
check: workspace-fmt-check workspace-lint workspace-test examples-run
