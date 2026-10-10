// Steady mixed recall/search load over MCP, with a light write trickle.
//   k6 run loadtest/k6/steady.js
import { sleep } from "k6";
import { setupUsers, pick, recall, search, write, mcpErrors } from "./lib.js";

export const options = {
  setupTimeout: "10m",
  scenarios: {
    steady: { executor: "constant-vus", vus: parseInt(__ENV.VUS || "10", 10), duration: __ENV.DURATION || "2m", exec: "mix" },
  },
  thresholds: {
    "http_req_duration{op:recall}": ["p(95)<150"],
    mcp_errors: ["rate<0.01"],
  },
};

export const setup = setupUsers;

export function mix(data) {
  const user = Math.random() < 0.7 ? pick(data, "small") : Math.random() < 0.5 ? pick(data, "mid") : pick(data, "whale");
  const r = Math.random();
  if (r < 0.6) recall(user);
  else if (r < 0.9) search(user);
  else write(user);
  sleep(0.2 + Math.random() * 0.3);
}
