import http from "k6/http";
import { check, sleep } from "k6";
import { crypto } from "k6/crypto";

export const options = {
  vus: 100, // Concurrent users
  duration: "1m",
  thresholds: {
    http_req_failed: ["rate<0.01"], // < 1% errors
    http_req_duration: ["p(95)<500"], // 95% of requests < 500ms
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:8080";

export default function () {
  // Generate a valid, signed payload (simplified)
  const payload = JSON.stringify({
    sender: "user1",
    receiver: "user2",
    amount: 100,
    timestamp: Date.now(),
  });

  const signature = crypto.hmac("sha256", "secret_key", payload, "hex");

  const params = {
    headers: {
      "Content-Type": "application/json",
      "X-Signature": signature,
    },
  };

  const res = http.post(`${BASE_URL}/transaction`, payload, params);

  check(res, {
    "status is 200": (r) => r.status === 200,
    "transaction accepted": (r) => r.json().status === "success",
  });

  sleep(1);
}

export function handleSummary(data) {
  return {
    stdout: textSummary(data, { indent: " ", enableColors: true }),
    "summary.json": JSON.stringify(data),
  };
}

import { textSummary } from "https://jslib.k6.io/k6-summary/0.0.2/index.js";
