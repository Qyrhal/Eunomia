"""FTS5 keyword index + sqlite-vec vector index for the cache.

Both are sqlite virtual tables kept in sync from Python (cache/search.py), not
by triggers — simpler to reason about at personal-data scale.
"""

from django.db import migrations

CREATE = [
    "CREATE VIRTUAL TABLE IF NOT EXISTS cache_record_fts "
    "USING fts5(record_id UNINDEXED, title, body_text, tokenize='porter unicode61')",
    "CREATE VIRTUAL TABLE IF NOT EXISTS cache_vec "
    "USING vec0(record_id TEXT PRIMARY KEY, embedding float[384])",
]
DROP = [
    "DROP TABLE IF EXISTS cache_record_fts",
    "DROP TABLE IF EXISTS cache_vec",
]


class Migration(migrations.Migration):
    dependencies = [("cache", "0001_initial")]

    operations = [
        migrations.RunSQL(sql=CREATE, reverse_sql=DROP),
    ]
