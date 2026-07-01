# Type-check a script (with host APIs)

## Purpose

Catch compile and type errors — including host API misuse — without running anything.

## Prerequisites

- For host-API checking, a `wscript.toml` listing your `.wscripti` interface files sits next to the scripts.

## Flowchart

![diagram](../_wdoc/process_check_script-diagram-1.svg)

## Steps

### Step 1: Check the script

```console
wscript check script.wscript
```

`check` honors `wscript.toml` and the `.wscripti` files it lists, so calls into registered host APIs are type-checked too.

### Step 2: Read the diagnostics

```text
error: expected `int`, found `string`  (script.wscript:4:18)
```

Every error carries a span and a help text. A non-zero exit code means the script did not type-check.

> [!TIP]
> **Verification**
> Exit code 0 and no diagnostics means the script (and its use of host APIs) type-checks.

## Related

- [The Type System](../references/concept_type_system.md)

- [Interface Files & wscript.toml](../references/concept_interface_files.md)

[← Back to SKILL.md](../SKILL.md)
