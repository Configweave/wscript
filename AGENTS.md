# AGENTS.md

Instructions for agents that work on wscript in Terrarium (project `wscript`).
Read `CLAUDE.md` for the language, the layout and the regeneration recipes.
Where `CLAUDE.md` describes the old flow (aciddog board, `.tree/` worktrees,
pull requests onto `main`), this file wins.

## Build and test

The sandbox has no Rust toolchain. Edit and run `git` in the sandbox, and build
and test on the target `wscript-helios-build`:

    terrarium exec wscript-helios-build -- sh -c 'cd /work/wscript/<agent>/wscript && just validate'

Do not run `git` on the target.

## Validation

- Card validation, run by the worker before `review`: `just validate`.
  It runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  then the affected tests from `.just/affected-tests.sh`: set
  `TERRARIUM_CHANGED_FILES` to the files your branch changed against `next`,
  for example

      TERRARIUM_CHANGED_FILES="$(git diff --name-only origin/next...HEAD)"

  (run the `git diff` in the sandbox and pass the list to the target). A change
  in a crate tests that crate and every crate that depends on it. Unset, it runs
  the full suite.
- Full suite, run by the merge queue on each batch: `just ci::check` (format,
  lint, every test, every example). It is the same gate that GitHub CI runs.
- A moved number in `tests/fuel.snap` or a changed snapshot is a behaviour
  change: say so in the `review` move comment.

## Flow

The project follows the Terrarium delivery process (skill `delivery-process`).

1. Branch from `next` with the card ID in lower case (`git checkout -b wsc-7 origin/next`).
   A bug card of a release branches from `release/<X.Y.Z>`.
2. Commit with conventional subjects (`feat(vm): ...`, `fix(parser): ...`).
   The subject type sets the release bump. Push the branch. Never push to
   `main`, `next` or a `release/*` branch.
3. Run `just validate` on the target against the pushed commit. Green: move the
   card to `review` with the branch, the commit, the command, the exit code and
   the pass count. Red: fix it, or move the card to `blocked` with the output tail.
4. The lead reviews and puts the card in `merge_queue`. The merge job merges the
   whole column onto `next`, runs `just ci::check`, and pushes `next` when green.
5. Releases: the lead cuts `release/<X.Y.Z>` from `next`, merges it into `main`
   with the `pre-release: true` trailer (CI publishes `vX.Y.Z-alpha`), and adds
   `release: true` only with the operator's approval.

No pull requests and no waiting on GitHub CI during card work.
