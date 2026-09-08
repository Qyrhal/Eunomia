"""Live agent tests: a real LLM (via hermes_sim) drives Eunomia's tools over the
REST surface and must answer real questions from real (masked) data.

Skipped unless RUN_LIVE_AGENT_TESTS=1, so `manage.py test` stays offline/hermetic
even with SIM_LLM_* in .env. Run explicitly:

    RUN_LIVE_AGENT_TESTS=1 uv run manage.py test tests_hermes_sim

The scenarios run in parallel (one agent per thread), each hitting the
LiveServerTestCase HTTP server — the way Hermes would.
"""

import json
import os
import re
import unittest
from concurrent.futures import ThreadPoolExecutor

from django.core.management import call_command
from django.test import LiveServerTestCase

import hermes_sim

RAW_PII = re.compile(r"\b[\w.+-]+@[\w-]+\.\w{2,}\b|\b\d{13,19}\b|\bsk-[A-Za-z0-9]{20,}\b")

SCENARIOS = [
    {
        "q": "How are the financials looking? Give me a total and the top spend category.",
        "expect_tools": {"up_bank__finance_summary", "search", "list"},
        "answer_has": re.compile(r"\$?\d"),
    },
    {
        "q": "What are my three biggest transactions? Just names and amounts.",
        "expect_tools": {"list", "search", "up_bank__finance_summary"},
        "answer_has": re.compile(r"\$?\d"),
    },
    {
        "q": "Do I have any meetings or calendar events coming up? Name one.",
        "expect_tools": {"search", "list"},
        "answer_has": re.compile(r"\w"),
    },
]


@unittest.skipUnless(
    os.environ.get("RUN_LIVE_AGENT_TESTS") == "1" and os.environ.get("SIM_LLM_API_KEY"),
    "set RUN_LIVE_AGENT_TESTS=1 (and SIM_LLM_* in .env) to run the live agent tests",
)
class HermesSimTests(LiveServerTestCase):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        from connectors.models import AppSettings

        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        call_command("seed_demo", "--seed", "5", verbosity=0)

    def _run(self, scenario):
        schemas, call = hermes_sim.rest_toolset(self.live_server_url)
        try:
            out = hermes_sim.run(scenario["q"], schemas, call, max_rounds=6)
        except Exception as e:  # a slow / flaky external LLM shouldn't crash the batch
            out = {"answer": "", "tool_calls": [], "transcript": [], "_error": str(e)}
        return scenario, out

    def test_agents_answer_from_tools_in_parallel(self):
        with ThreadPoolExecutor(max_workers=len(SCENARIOS)) as ex:
            results = list(ex.map(self._run, SCENARIOS))

        passed = 0
        for scenario, out in results:
            with self.subTest(q=scenario["q"]):
                if out.get("_error"):
                    print(f"  [skipped: LLM error] {scenario['q']} -> {out['_error'][:120]}")
                    continue
                tools_used = {name for name, _, _ in out["tool_calls"]}
                self.assertTrue(out["tool_calls"], f"no tool calls for: {scenario['q']}")
                self.assertTrue(
                    tools_used & scenario["expect_tools"],
                    f"{scenario['q']!r} used {tools_used}, expected one of {scenario['expect_tools']}",
                )
                self.assertRegex(out["answer"], scenario["answer_has"])
                blob = json.dumps(out["transcript"], default=str)
                leak = RAW_PII.search(blob)
                self.assertIsNone(leak, f"raw value leaked to the agent: {leak and leak.group(0)!r}")
                passed += 1

        # the point of the test: real agents, in parallel, actually get through.
        self.assertGreaterEqual(passed, 2, f"only {passed}/{len(SCENARIOS)} scenarios completed cleanly")
