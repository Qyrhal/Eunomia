"""Re-embed every cache record (and, once #38 lands, every task) with the
currently-configured backend. Run after switching `embedding_backend` or model —
stored vectors from a different model are not comparable.
"""

from django.core.management.base import BaseCommand
from django.db import connection

from cache.models import CacheRecord
from cache.search import set_embedding
from embeddings.service import embed


class Command(BaseCommand):
    help = "Rebuild all cache embeddings with the configured backend."

    def add_arguments(self, parser):
        parser.add_argument("--batch", type=int, default=64)

    def handle(self, *args, **opts):
        batch = opts["batch"]
        with connection.cursor() as cur:
            cur.execute("DELETE FROM cache_vec")

        qs = CacheRecord.objects.exclude(body_text="").filter(deleted=False)
        total = qs.count()
        done = 0
        rows = list(qs.only("id", "title", "body_text"))
        for i in range(0, len(rows), batch):
            chunk = rows[i : i + batch]
            vecs = embed([f"{r.title}\n{r.body_text}" for r in chunk])
            for r, v in zip(chunk, vecs):
                set_embedding(r.id, v)
            done += len(chunk)
            self.stdout.write(f"  {done}/{total}")
        self.stdout.write(self.style.SUCCESS(f"Re-embedded {done} records."))
