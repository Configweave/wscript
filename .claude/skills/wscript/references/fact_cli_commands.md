# The `wscript` CLI

The `wscript` binary compiles and runs scripts, type-checks them, drops into an interactive REPL, and serves a language server. `wscript run` enables the full standard library; execution starts at `fn main()` and its `int` return becomes the process exit code (`unit` means 0).

| Command | Purpose | Example |
| --- | --- | --- |
| `run <script.wscript> [args…]` | Compile and execute a script with the full stdlib. Arguments after the path are read with `process::args()`. | `wscript run script.wscript --verbose 3` |
| `check <script.wscript>` | Compile and type-check without running. Honors `wscript.toml` and the `.wscripti` interfaces it lists, so host APIs are checked too. Non-zero exit on any error. | `wscript check script.wscript` |
| `repl` | Interactive read-eval-print loop with bindings persisted across lines. | `wscript repl` |
| `lsp` | Language server over stdio: diagnostics, hover, go-to-definition, completions. Reads `wscript.toml` / `.wscripti` for registered host APIs. | `wscript lsp` |

## Related

- [Run a wscript script](../references/process_run_script.md)

- [Type-check a script (with host APIs)](../references/process_check_script.md)

- [Interface Files & wscript.toml](../references/concept_interface_files.md)

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
