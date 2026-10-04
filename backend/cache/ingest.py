"""The ingest pipeline: raw source record -> cache row.

One entrypoint, :func:`ingest`, called by the scheduler, webhook endpoints, and
on-demand refresh. Stages, per record: map -> upsert (idempotent) -> embed. No
masking/PII stage (dropped per the FastAPI+SurrealDB rewrite plan).

Partial failure is isolated: a bad record is recorded and skipped, the batch
continues. Embedding failure is non-fatal (backfill retries).
"""

from dataclasses import dataclass, field


@dataclass
class IngestReport:
    source: str
    written: int = 0  # new or changed records persisted
    skipped: int = 0  # unchanged, or mapper returned None
    failed: int = 0  # raised an exception
    errors: list[str] = field(default_factory=list)

    def as_dict(self):
        return {
            "source": self.source,
            "written": self.written,
            "skipped": self.skipped,
            "failed": self.failed,
            "errors": self.errors[:20],
        }


async def _embed_record(owner, rec) -> None:
    from cache.search import set_embedding
    from embeddings.service import embed

    text = f"{rec.title}\n{rec.body_text}".strip()
    if not text:
        return
    vec = (await embed([text]))[0]
    await set_embedding(owner, rec.id, vec)


async def ingest(owner, source_key: str, raw_records, map_fn) -> IngestReport:
    from cache.search import upsert

    report = IngestReport(source=source_key)
    for raw in raw_records:
        try:
            env = map_fn(raw)
            if env is None:
                report.skipped += 1
                continue
            env.setdefault("source", source_key)

            rec, changed = await upsert(owner, env)
            if not changed:
                report.skipped += 1
                continue
            report.written += 1

            if not rec.deleted:
                try:
                    await _embed_record(owner, rec)
                except Exception as e:  # non-fatal -- backfill will retry
                    report.errors.append(f"embed {rec.id}: {e}")
        except Exception as e:
            report.failed += 1
            report.errors.append(
                f"{getattr(raw, 'get', lambda *_: '?')('id') if isinstance(raw, dict) else '?'}: {e}"
            )
    return report
