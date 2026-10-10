# Architecture diagram

- `architecture.json`: editable Archify specification.
- `architecture.html`: standalone interactive viewer; open locally.
- `architecture.svg`: static, self-contained README image with computed styles.
- `architecture.visual-check.*`: desktop containment receipt and review captures.

The diagram shows the 2.0 layout: the API and job workers (one backend image,
`EUNOMIA_ROLE`) over SurrealDB 3.3 with a `control` database and one database
per org.

Validation: 9/9 showcase checks, zero errors and warnings.
Visual review: passed in light and dark; containment passed at 1440×900,
1600×1000, 1920×1080 and 2048×1320. The static SVG was also rendered and
inspected.

Specification SHA-256:
`a6342d8ae0454e6248fbf6129022255b4a3339b14394b19b83c397bed7a75b80`

Interactive artifact SHA-256:
`f49ad50f67d2f9b1feab7c54bbe3315a9f5d9256f29c829a9285455197cd969b`

Regenerate using the installed archify skill:

```bash
node /path/to/archify/bin/archify.mjs validate architecture docs/diagrams/architecture.json --quality showcase --json
node /path/to/archify/bin/archify.mjs deliver architecture docs/diagrams/architecture.json docs/diagrams/architecture.html --quality showcase --json
node /path/to/archify/bin/archify.mjs visual-check docs/diagrams/architecture.html --json
```

After regeneration, export a fresh SVG and refresh the receipts. The SVG is
the viewer's rendered diagram in the light theme with each element's computed
styles inlined (the viewer's Export menu gives an equivalent, theme-aware
file). It is an export, not a separate architecture spec.
