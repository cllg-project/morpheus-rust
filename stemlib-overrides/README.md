# stemlib-overrides

Custom lemma entries that extend the upstream stemlib. Files live in
`<Lang>/stemsrc/` in the standard stem-source format (beta-code,
`:le:`/`:no:`/`:vs:`/`:de:`/`:wd:` lines) and are loaded *after* the
upstream files — the overlay is additive (it cannot suppress upstream
entries).

The easiest way to edit this directory is the web UI:

```bash
morpheus edit -m /path/to/stemlib --overlay stemlib-overrides
# then open http://127.0.0.1:8788/
```

It writes nominal entries to `Greek/stemsrc/custom.nom` and verbal ones to
`Greek/stemsrc/custom.vbs`, with validation and a live paradigm preview.
Hand-editing those files works too.

Use the overlay at analysis time with:

```bash
morpheus -m /path/to/stemlib --overlay stemlib-overrides <words>
# or: export MORPHEUS_OVERLAY=stemlib-overrides
```

```python
parser = morpheus.Parser("/path/to/stemlib", overlay_path="stemlib-overrides")
```
