"""The ingest pipeline (#23): raw source record -> masked, embedded cache row.

One entrypoint, :func:`ingest`, called by the scheduler (#34), webhook endpoints
(#35), and on-demand refresh. Stages, per record:

  map -> credential-mask -> PII tokenize -> upsert (idempotent) -> embed -> link
  -> trigger-rule eval

Partial failure is isolated: a bad record is recorded and skipped, the batch
continues. Embedding failure is non-fatal (backfill retries).
"""

from dataclasses import dataclass, field

from masking.vault import VaultSecret, tokenize, tokenize_text


@dataclass
class IngestReport:
    source: str
    written: int = 0     # new or changed records persisted
    skipped: int = 0     # unchanged, or mapper returned None
    failed: int = 0      # raised an exception
    errors: list[str] = field(default_factory=list)

    def as_dict(self):
        return {
            "source": self.source,
            "written": self.written,
            "skipped": self.skipped,
            "failed": self.failed,
            "errors": self.errors[:20],
        }


def _mask_strings(obj, fn):
    if isinstance(obj, str):
        return fn(obj)
    if isinstance(obj, dict):
        return {k: _mask_strings(v, fn) for k, v in obj.items()}
    if isinstance(obj, list):
        return [_mask_strings(v, fn) for v in obj]
    return obj


def _apply_credentials(env: dict, secret_values: list[str], source: str):
    if not secret_values:
        return env
    pairs = [(v, tokenize(v, "credential", kind=VaultSecret.KIND_CREDENTIAL, source=source))
             for v in secret_values if v]

    def repl(s: str) -> str:
        for raw, tok in pairs:
            if raw and raw in s:
                s = s.replace(raw, tok)
        return s

    for key in ("title", "body_text"):
        if env.get(key):
            env[key] = repl(env[key])
    env["payload"] = _mask_strings(env.get("payload", {}), repl)
    return env


def _apply_pii(env: dict, source: str):
    for key in ("title", "body_text"):
        if env.get(key):
            env[key] = tokenize_text(env[key], source=source)
    env["payload"] = _mask_strings(env.get("payload", {}), lambda s: tokenize_text(s, source=source))
    return env


def _embed_record(rec):
    from cache.search import set_embedding
    from embeddings.service import embed

    text = f"{rec.title}\n{rec.body_text}".strip()
    if not text:
        return
    set_embedding(rec.id, embed([text])[0])


def _eval_rules(rec):
    try:
        from triggers.rules import evaluate_record
    except ImportError:
        return
    evaluate_record(rec)


def ingest(source_key: str, raw_records, map_fn, *, secret_values=None) -> IngestReport:
    from cache.search import upsert

    report = IngestReport(source=source_key)
    for raw in raw_records:
        try:
            env = map_fn(raw)
            if env is None:
                report.skipped += 1
                continue
            env.setdefault("source", source_key)
            _apply_credentials(env, secret_values or [], source_key)
            _apply_pii(env, source_key)

            rec, changed = upsert(env)
            if not changed:
                report.skipped += 1
                continue
            report.written += 1

            if not rec.deleted:
                try:
                    _embed_record(rec)
                except Exception as e:  # non-fatal — backfill will retry
                    report.errors.append(f"embed {rec.id}: {e}")
                _eval_rules(rec)
        except Exception as e:
            report.failed += 1
            report.errors.append(f"{getattr(raw, 'get', lambda *_: '?')('id') if isinstance(raw, dict) else '?'}: {e}")
    return report
