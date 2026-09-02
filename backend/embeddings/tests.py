from unittest.mock import patch

from django.test import TestCase

from .models import EmbedCache
from .service import DIM, embed


class EmbedStubTests(TestCase):
    def test_stub_returns_dim_sized_vectors_in_order(self):
        vs = embed(["alpha", "beta", "gamma"])
        self.assertEqual(len(vs), 3)
        self.assertTrue(all(len(v) == DIM for v in vs))
        self.assertNotEqual(vs[0], vs[1])

    def test_deterministic(self):
        self.assertEqual(embed(["same"]), embed(["same"]))

    def test_empty(self):
        self.assertEqual(embed([]), [])

    def test_memo_hit_skips_recompute(self):
        embed(["cached text"])
        self.assertEqual(EmbedCache.objects.count(), 1)
        with patch("embeddings.service._embed_stub") as m:
            embed(["cached text"])
            m.assert_not_called()

    def test_partial_memo(self):
        embed(["one"])
        with patch("embeddings.service._embed_stub", wraps=__import__("embeddings.service", fromlist=["_embed_stub"])._embed_stub) as m:
            out = embed(["one", "two"])
            # only the uncached "two" is computed
            m.assert_called_once_with(["two"])
        self.assertEqual(len(out), 2)
        self.assertEqual(EmbedCache.objects.count(), 2)
