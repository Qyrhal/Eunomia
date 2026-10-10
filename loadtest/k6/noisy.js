// Noisy neighbour: small users run steady load alone (baseline), then again while the whale hammers.
// Fairness = small-user recall p95 while noisy stays under 2x the baseline.
//   k6 run loadtest/k6/noisy.js
//   k6 run -e BASELINE_P95_MS=60 loadtest/k6/noisy.js   (pin the baseline from an earlier run)
import { sleep } from "k6";
import { setupUsers, pick, recall, search, fairnessSummary } from "./lib.js";
import { textSummary } from "https://jslib.k6.io/k6-summary/0.0.4/index.js";

const PHASE = __ENV.PHASE_SECONDS || "60";
const gap = `${parseInt(PHASE, 10) + 10}s`; // baseline, then a 10s breather, then the noisy phase
// Defaults to 75 so the noisy limit equals the 150 ms absolute recall target.
const BASELINE = parseFloat(__ENV.BASELINE_P95_MS || "75");

export const options = {
  setupTimeout: "10m",
  scenarios: {
    baseline: { executor: "constant-vus", vus: 5, duration: `${PHASE}s`, exec: "small", tags: { phase: "baseline" } },
    small_noisy: { executor: "constant-vus", vus: 5, duration: `${PHASE}s`, startTime: gap, exec: "small", tags: { phase: "noisy" } },
    whale: { executor: "constant-vus", vus: parseInt(__ENV.WHALE_VUS || "20", 10), duration: `${PHASE}s`, startTime: gap, exec: "whale", tags: { phase: "noisy" } },
  },
  thresholds: {
    "http_req_duration{op:recall,size:small,phase:baseline}": ["p(95)<150"],
    [`http_req_duration{op:recall,size:small,phase:noisy}`]: [`p(95)<${2 * BASELINE}`],
    "http_req_duration{op:recall,size:mid,phase:noisy}": [`p(95)<${2 * BASELINE}`],
    mcp_errors: ["rate<0.01"],
  },
};

export const setup = setupUsers;

export function small(data) {
  recall(pick(data, "small"));
  if (Math.random() < 0.3) search(pick(data, "small"));
  sleep(0.2 + Math.random() * 0.3);
}

// No sleep: the biggest user hits recall and search as fast as it can.
export function whale(data) {
  const u = pick(data, "whale");
  if (Math.random() < 0.7) recall(u);
  else search(u);
}

export function handleSummary(data) {
  return fairnessSummary(data, textSummary(data, { indent: " ", enableColors: false }));
}
