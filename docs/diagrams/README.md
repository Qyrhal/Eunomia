# Architecture diagram

- `architecture.json`: editable Archify specification.
- `architecture.html`: standalone interactive viewer; open locally.
- `architecture.svg`: static, self-contained README image with computed styles.
- `architecture.visual-check.*`: desktop containment receipt and review captures.

Validation: 9/9 showcase checks, zero errors and warnings.
Visual review: passed in light and dark; containment passed at 1440×900,
1600×1000, 1920×1080 and 2048×1320. One label correction was made.
The static SVG was also rendered and inspected.

Specification SHA-256:
`d6a4b2c1cb929568f8da4d1282deae5da7733e1c1ce56ffd0c93ff3db2edf908`

Interactive artifact SHA-256:
`dc66cc6b0b7ab2d966e84f2a25480e80518182b6c33d8742538a61dd54a0c45f`

Regenerate using the installed archify skill:

```bash
node /path/to/archify/bin/archify.mjs validate architecture docs/diagrams/architecture.json --quality showcase --json
node /path/to/archify/bin/archify.mjs deliver architecture docs/diagrams/architecture.json docs/diagrams/architecture.html --quality showcase --json
node /path/to/archify/bin/archify.mjs visual-check docs/diagrams/architecture.html --json
```

After regeneration, export a fresh SVG from the viewer's Export menu and
refresh the receipts. The SVG is an export, not a separate architecture spec.
