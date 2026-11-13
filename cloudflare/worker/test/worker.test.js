import { describe, it } from "node:test";
import assert from "node:assert/strict";
import worker from "../src/index.js";

function createMockKv(initial = {}) {
  const store = new Map(Object.entries(initial));
  return {
    get: async (key) => store.get(key) ?? null,
    put: async (key, val) => {
      store.set(key, String(val));
    },
    delete: async (key) => store.delete(key),
    _store: store,
  };
}

const DEFAULT_ENV = {
  UPDATE_SECRET: "test-update-secret-12345",
  TERMINAL_SECRET: "test-terminal-secret-67890",
  TARGET_ORIGIN: "https://magpie.backend.local",
  TERMINAL_ORIGIN: "https://terminal.backend.local",
};

const DISCORD_PROXY_HEADERS = {
  "X-Forwarded-Host": "1234567890.discordsays.com",
  "User-Agent": "Mozilla/5.0 (Discord Activity Proxy)",
};

describe("Cloudflare Worker Activity Switcher Tests", () => {
  describe("1. Mode Updates (POST /__set_launch_mode) - Success", () => {
    it("successfully sets terminal mode for a channel in KV", async () => {
      const kv = createMockKv();
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };

      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({
          channel_id: "123456789012345678",
          mode: "terminal",
        }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 200);
      assert.equal(res.headers.get("Cache-Control"), "no-store");

      const data = await res.json();
      assert.deepEqual(data, {
        success: true,
        channel_id: "123456789012345678",
        mode: "terminal",
      });

      assert.equal(await kv.get("channel_mode:123456789012345678"), "terminal");
      // Must not write global fallback
      assert.equal(await kv.get("last_launch_mode"), null);
    });

    it("successfully sets magpie mode for a channel in KV", async () => {
      const kv = createMockKv({ "channel_mode:123456789012345678": "terminal" });
      const env = { ...DEFAULT_ENV, MAGPIE_KV: kv };

      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({
          channel_id: "123456789012345678",
          mode: "magpie",
        }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 200);
      assert.equal(res.headers.get("Cache-Control"), "no-store");

      const data = await res.json();
      assert.deepEqual(data, {
        success: true,
        channel_id: "123456789012345678",
        mode: "magpie",
      });

      assert.equal(await kv.get("channel_mode:123456789012345678"), "magpie");
      assert.equal(await kv.get("last_launch_mode"), null);
    });

    it("trims whitespace in channel_id upon mode update", async () => {
      const kv = createMockKv();
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };

      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({
          channel_id: "   123456789012345678   ",
          mode: "terminal",
        }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 200);
      const data = await res.json();
      assert.equal(data.channel_id, "123456789012345678");
      assert.equal(await kv.get("channel_mode:123456789012345678"), "terminal");
    });
  });

  describe("2. Mode Updates - Missing & Invalid Inputs", () => {
    it("rejects non-POST methods with 405", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "GET",
        headers: { Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}` },
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 405);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects invalid JSON body with 400", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: "{not-valid-json}",
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 400);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects missing channel_id with 400", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 400);
      const text = await res.text();
      assert.match(text, /channel_id/i);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects non-numeric channel_id with 400", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ channel_id: "invalid-id-abc", mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 400);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects missing mode with 400", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ channel_id: "123456789012345678" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 400);
      const text = await res.text();
      assert.match(text, /mode/i);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects invalid mode name with 400", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ channel_id: "123456789012345678", mode: "other_app" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 400);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });
  });

  describe("3. Authentication Rejection", () => {
    it("rejects request without Authorization header with 401", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ channel_id: "123456789012345678", mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 401);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects request with incorrect Bearer token with 401", async () => {
      const env = { ...DEFAULT_ENV, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: "Bearer wrong-token",
        },
        body: JSON.stringify({ channel_id: "123456789012345678", mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 401);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("rejects request when UPDATE_SECRET is unset in worker env", async () => {
      const env = { ...DEFAULT_ENV, UPDATE_SECRET: undefined, TERMINAL_KV: createMockKv() };
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ channel_id: "123456789012345678", mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 401);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });
  });

  describe("4. Storage Failure Handling", () => {
    it("returns 503 when neither TERMINAL_KV nor MAGPIE_KV is bound", async () => {
      const env = { ...DEFAULT_ENV }; // no KV bindings
      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ channel_id: "123456789012345678", mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 503);
      const text = await res.text();
      assert.match(text, /Mode store unavailable/i);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("returns 500 when KV put operation throws an error", async () => {
      const failingKv = {
        get: async () => null,
        put: async () => {
          throw new Error("KV cluster write timeout");
        },
      };
      const env = { ...DEFAULT_ENV, TERMINAL_KV: failingKv };

      const req = new Request("https://worker.local/__set_launch_mode", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${DEFAULT_ENV.UPDATE_SECRET}`,
        },
        body: JSON.stringify({ channel_id: "123456789012345678", mode: "terminal" }),
      });

      const res = await worker.fetch(req, env);
      assert.equal(res.status, 500);
      const text = await res.text();
      assert.match(text, /Failed to persist launch mode/i);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });
  });

  describe("5. Activity Routing & Cache Headers", () => {
    it("blocks direct internet access with 403 and no-store header", async () => {
      const req = new Request("https://worker.local/");
      const res = await worker.fetch(req, DEFAULT_ENV);
      assert.equal(res.status, 403);
      assert.equal(res.headers.get("Cache-Control"), "no-store");
    });

    it("routes /terminal path to terminal origin deterministically", async () => {
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;
      let requestedHeaders = null;

      globalThis.fetch = async (url, opts) => {
        requestedUrl = url;
        requestedHeaders = opts.headers;
        return new Response("<html>Terminal UI</html>", {
          status: 200,
          headers: { "Content-Type": "text/html; charset=utf-8" },
        });
      };

      try {
        const req = new Request("https://worker.local/terminal", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, DEFAULT_ENV);

        assert.equal(res.status, 200);
        assert.match(requestedUrl, /^https:\/\/terminal\.backend\.local\/terminal/);
        assert.equal(
          requestedHeaders.get("Authorization"),
          `Bearer ${DEFAULT_ENV.TERMINAL_SECRET}`
        );
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("routes /ws path to terminal origin deterministically", async () => {
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("OK", { status: 200, headers: { "Content-Type": "text/plain" } });
      };

      try {
        const req = new Request("https://worker.local/ws", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, DEFAULT_ENV);

        assert.equal(res.status, 200);
        assert.match(requestedUrl, /^https:\/\/terminal\.backend\.local\/ws/);
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("routes ?mode=terminal override to terminal origin", async () => {
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("<html>Terminal</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      };

      try {
        const req = new Request("https://worker.local/?mode=terminal", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, DEFAULT_ENV);

        assert.equal(res.status, 200);
        assert.match(requestedUrl, /^https:\/\/terminal\.backend\.local\/terminal\//);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("routes ?mode=magpie override to magpie origin even if channel_mode is terminal", async () => {
      const kv = createMockKv({ "channel_mode:123456789012345678": "terminal" });
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("<html>Magpie UI</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      };

      try {
        const req = new Request("https://worker.local/?channel_id=123456789012345678&mode=magpie", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, env);

        assert.equal(res.status, 200);
        assert.match(requestedUrl, /^https:\/\/magpie\.backend\.local\//);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("routes channel to terminal when channel_mode in KV is terminal", async () => {
      const kv = createMockKv({ "channel_mode:123456789012345678": "terminal" });
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("<html>Terminal</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      };

      try {
        const req = new Request("https://worker.local/?channel_id=123456789012345678", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, env);

        assert.equal(res.status, 200);
        assert.match(requestedUrl, /^https:\/\/terminal\.backend\.local\/terminal\//);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("routes channel to terminal even when channel_id in query has whitespace", async () => {
      const kv = createMockKv({ "channel_mode:123456789012345678": "terminal" });
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("<html>Terminal</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      };

      try {
        const req = new Request("https://worker.local/?channel_id=%20123456789012345678%20", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, env);

        assert.equal(res.status, 200);
        assert.match(requestedUrl, /^https:\/\/terminal\.backend\.local\/terminal\//);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("enforces Cache-Control: no-store on mixed-case Text/HTML content type", async () => {
      const originalFetch = globalThis.fetch;

      globalThis.fetch = async () => {
        return new Response("<html>Activity</html>", {
          status: 200,
          headers: { "Content-Type": "Text/HTML; charset=UTF-8" },
        });
      };

      try {
        const req = new Request("https://worker.local/terminal", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, DEFAULT_ENV);

        assert.equal(res.status, 200);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("enforces Cache-Control: no-store when backend is unreachable (502)", async () => {
      const originalFetch = globalThis.fetch;

      globalThis.fetch = async () => {
        throw new Error("Connection refused to target backend");
      };

      try {
        const req = new Request("https://worker.local/terminal", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, DEFAULT_ENV);

        assert.equal(res.status, 502);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("isolates channels: Channel B does NOT inherit Channel A terminal mode", async () => {
      // Channel A has terminal mode set
      const kv = createMockKv({ "channel_mode:11111111111111111": "terminal" });
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("<html>Magpie</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      };

      try {
        // Channel B opens activity root
        const req = new Request("https://worker.local/?channel_id=22222222222222222", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, env);

        assert.equal(res.status, 200);
        // Channel B must route to Magpie (default), NOT Channel A's terminal!
        assert.match(requestedUrl, /^https:\/\/magpie\.backend\.local\//);
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it("eventual consistency race simulation: unpropagated KV read safely defaults to Magpie", async () => {
      // Simulate KV replication delay where key is not yet present in this edge colocation
      const kv = createMockKv({}); // empty
      const env = { ...DEFAULT_ENV, TERMINAL_KV: kv };
      const originalFetch = globalThis.fetch;
      let requestedUrl = null;

      globalThis.fetch = async (url) => {
        requestedUrl = url;
        return new Response("<html>Magpie</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      };

      try {
        const req = new Request("https://worker.local/?channel_id=99999999999999999", {
          headers: DISCORD_PROXY_HEADERS,
        });
        const res = await worker.fetch(req, env);

        assert.equal(res.status, 200);
        // Graceful fallback to Magpie default without throwing or cross-contaminating
        assert.match(requestedUrl, /^https:\/\/magpie\.backend\.local\//);
        assert.equal(res.headers.get("Cache-Control"), "no-store");
      } finally {
        globalThis.fetch = originalFetch;
      }
    });
  });
});
