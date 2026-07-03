# wscript — wskill

A **wskill**: one self-contained folder capturing everything about *wscript*
— reference, processes, and curated indexes — as a single WCL data model, projected into
a human-readable book, a Claude Code skill, an overview deck, and a training course.

## Layout

```
wskill.wcl               # entry point: topic, version pin, meta, artifacts, sources, data imports
schema/base.wcl          # base block types (DO NOT hand-edit)
schema/kinds.wcl         # topic-owned vocabularies (EntityKind, ArtifactKind) — hand-editable
schema/extensions.wcl    # custom block types for this topic
schema/presentation.wcl  # opt-in schema for the overview deck
schema/training.wcl      # opt-in schema for the tutorial series
data/                    # the content: reference / processes / presentation / training
wdoc/book, wdoc/skill    # projection templates (no content — pure structure)
wdoc/presentation        # overview-deck projection template
wdoc/training            # training-course projection template
out/                     # generated outputs (gitignored)
```

## Build

```bash
just                    # list recipes
just wskill-check       # validate against the schema
just render             # build out/book, out/skill, out/presentation and out/training
just book-serve         # live-preview the book
```

Install the rendered skill into a repo by copying it:

```bash
cp -r out/skill <repo>/.claude/skills/<name>
```

## Editing

Add content by writing block instances into `data/`. The templates project them
automatically — never hand-edit `out/`. Keep `wskill-check` green and re-render
after changes.
