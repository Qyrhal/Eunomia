// Write-heavy burst: ramps writes hard, with a small recall probe running alongside.
//   k6 run loadtest/k6/burst.js
import { sleep } from "k6";
import { setupUsers, pick, recall, write } from "./lib.js";

export const options = {
  setupTimeout: "10m",
  scenarios: {
    burst: {
      executor: "ramping-arrival-rate",
      startRate: 5,
      timeUnit: "1s",
      preAllocatedVUs: 50,
      maxVUs: 200,
      stages: [
        { target: parseInt(__ENV.PEAK_RPS || "60", 10), duration: "30s" },
        { target: parseInt(__ENV.PEAK_RPS || "60", 10), duration: "1m" },
        { target: 5, duration: "20s" },
      ],
      exec: "writer",
    },
    probe: { executor: "constant-vus", vus: 2, duration: "1m50s", exec: "probe" },
  },
  thresholds: {
    // writes may be slower than recall, but must stay bounded and not error
    "http_req_duration{op:write}": ["p(95)<1000"],
    "http_req_duration{op:recall}": ["p(95)<150"],
    mcp_errors: ["rate<0.01"],
  },
};

export const setup = setupUsers;

export function writer(data) {
  const tier = Math.random() < 0.5 ? "small" : Math.random() < 0.5 ? "mid" : "whale";
  write(pick(data, tier));
}

export function probe(data) {
  recall(pick(data, "small"));
  sleep(0.3);
}
