/**
 * Cloudflare Worker: Discord Embedded Activity Reverse Proxy (Magpie Web UI & VM Terminal)
 *
 * Provides a permanent HTTPS entrypoint (*.workers.dev) for Discord Activities.
 *
 * Security:
 * - Direct internet access is unconditionally blocked (HTTP 403).
 * - Master session cookie (MAGPIE_COOKIE) is only injected into Discord proxy traffic for Magpie.
 * - Terminal requests (/terminal/*, /ws) route to the terminal bridge with Authorization: Bearer <TERMINAL_SECRET>.
 * - Optional cryptographic Ed25519 verification when DISCORD_PUBLIC_KEY is configured.
 * - Strips iframe-blocking headers (X-Frame-Options, Content-Security-Policy).
 */

function hexToBytes(hex) {
  const clean = (hex || "").trim();
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(clean.substring(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

// Verify Ed25519 signatures (Discord Proxy Authentication or HTTP Interactions)
async function verifyDiscordSignature(request, rawBodyBuffer, publicKeyHex) {
  const signature = request.headers.get("X-Signature-Ed25519");
  const timestamp = request.headers.get("X-Signature-Timestamp");
  const proxyPayload = request.headers.get("X-Discord-Proxy-Payload");

  if (!signature || !timestamp || !publicKeyHex) return false;

  try {
    const keyBytes = hexToBytes(publicKeyHex);
    const sigBytes = hexToBytes(signature);
    if (keyBytes.length !== 32 || sigBytes.length !== 64) return false;

    const cryptoKey = await crypto.subtle.importKey(
      "raw",
      keyBytes,
      { name: "Ed25519" },
      false,
      ["verify"]
    );

    const encoder = new TextEncoder();
    const timestampBytes = encoder.encode(timestamp);

    let payloadBytes;
    if (proxyPayload) {
      try {
        const bin = atob(proxyPayload);
        payloadBytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
      } catch (_) {
        payloadBytes = encoder.encode(proxyPayload);
      }
    } else if (rawBodyBuffer && rawBodyBuffer.byteLength > 0) {
      payloadBytes = new Uint8Array(rawBodyBuffer);
    } else {
      payloadBytes = new Uint8Array(0);
    }

    const merged = new Uint8Array(timestampBytes.length + payloadBytes.length);
    merged.set(timestampBytes, 0);
    merged.set(payloadBytes, timestampBytes.length);

    return await crypto.subtle.verify({ name: "Ed25519" }, cryptoKey, sigBytes, merged);
  } catch (_) {
    return false;
  }
}

// Check if an incoming request genuinely originates from Discord's Activity Proxy
function isDiscordRequest(request) {
  const cfWorker = request.headers.get("cf-worker") || "";
  const fwdHost = request.headers.get("X-Forwarded-Host") || "";
  const fwd = request.headers.get("Forwarded") || "";
  const via = request.headers.get("Via") || "";
  const referer = request.headers.get("Referer") || "";
  const origin = request.headers.get("Origin") || "";
  const userAgent = request.headers.get("User-Agent") || "";

  return (
    cfWorker.includes("discordsays.com") ||
    fwdHost.includes(".discordsays.com") ||
    fwd.includes(".discordsays.com") ||
    referer.includes(".discordsays.com") ||
    referer.includes("discord.com") ||
    origin.includes("discordsays.com") ||
    origin.includes("discord.com") ||
    via.toLowerCase().includes("discord") ||
    userAgent.toLowerCase().includes("discord") ||
    request.headers.has("X-Discord-Proxy-Payload") ||
    request.headers.has("X-Signature-Ed25519")
  );
}

// RFC 6455 compliant WebSocket close helper that ignores invalid/reserved status codes (e.g. 1005, 1006)
function safeCloseWs(ws, code, reason) {
  try {
    if (!ws) return;
    if (code && code >= 1000 && code < 5000 && code !== 1004 && code !== 1005 && code !== 1006 && code !== 1015) {
      ws.close(code, reason || "");
    } else {
      ws.close(1000, "Normal closure");
    }
  } catch (_) {
    try { ws.close(); } catch (_) {}
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    // 1. Health check (GET /__status)
    if (url.pathname === "/__status") {
      const activeOrigin = (await env.MAGPIE_KV?.get("origin_url")) || env.TARGET_ORIGIN;
      const hasActiveCookie = Boolean((await env.MAGPIE_KV?.get("active_cookie")) || env.MAGPIE_COOKIE);
      const terminalOrigin = (await env.TERMINAL_KV?.get("origin_url"))
        || (await env.MAGPIE_KV?.get("terminal_origin_url"))
        || env.TERMINAL_ORIGIN;
      return Response.json({
        status: "healthy",
        worker: "magpie-activity",
        active_backend_origin: activeOrigin,
        active_cookie_synced: hasActiveCookie,
        terminal_backend_origin: terminalOrigin || null,
        terminal_backend_configured: Boolean(terminalOrigin),
        ed25519_verification_enforced: Boolean(env.DISCORD_PUBLIC_KEY),
        timestamp: new Date().toISOString(),
      });
    }

    // 2a. Self-healing origin & cookie update from VM daemon for Magpie (POST /__update_origin)
    if (url.pathname === "/__update_origin") {
      if (request.method !== "POST") return new Response("Method Not Allowed", { status: 405 });

      const auth = request.headers.get("Authorization");
      if (!env.UPDATE_SECRET || auth !== `Bearer ${env.UPDATE_SECRET}`) {
        return new Response("Unauthorized", { status: 401 });
      }

      try {
        const { origin, cookie } = await request.json();
        if (!origin || !origin.startsWith("http")) return new Response("Invalid origin", { status: 400 });

        if (env.MAGPIE_KV) {
          await env.MAGPIE_KV.put("origin_url", origin);
          if (cookie && typeof cookie === "string" && cookie.startsWith("magpie_web_")) {
            await env.MAGPIE_KV.put("active_cookie", cookie);
          }
        }
        return Response.json({
          success: true,
          updated_origin: origin,
          updated_cookie: Boolean(cookie),
        });
      } catch (err) {
        return new Response(`Bad request: ${err.message}`, { status: 400 });
      }
    }

    // 2b. Self-healing origin update from VM daemon for Terminal Bridge (POST /__update_terminal_origin)
    if (url.pathname === "/__update_terminal_origin") {
      if (request.method !== "POST") return new Response("Method Not Allowed", { status: 405 });

      const auth = request.headers.get("Authorization");
      if (!env.UPDATE_SECRET || auth !== `Bearer ${env.UPDATE_SECRET}`) {
        return new Response("Unauthorized", { status: 401 });
      }

      try {
        const { origin } = await request.json();
        if (!origin || !origin.startsWith("http")) return new Response("Invalid origin", { status: 400 });

        if (env.TERMINAL_KV) {
          await env.TERMINAL_KV.put("origin_url", origin);
        } else if (env.MAGPIE_KV) {
          await env.MAGPIE_KV.put("terminal_origin_url", origin);
        }
        return Response.json({
          success: true,
          updated_origin: origin,
        });
      } catch (err) {
        return new Response(`Bad request: ${err.message}`, { status: 400 });
      }
    }

    // 2c. Update active activity mode for channel (POST /__set_launch_mode)
    if (url.pathname === "/__set_launch_mode") {
      if (request.method !== "POST") {
        return new Response("Method Not Allowed", {
          status: 405,
          headers: { "Cache-Control": "no-store" },
        });
      }

      const auth = request.headers.get("Authorization");
      if (!env.UPDATE_SECRET || auth !== `Bearer ${env.UPDATE_SECRET}`) {
        return new Response("Unauthorized", {
          status: 401,
          headers: { "Cache-Control": "no-store" },
        });
      }

      let payload;
      try {
        payload = await request.json();
      } catch (err) {
        return new Response(`Bad request: ${err.message}`, {
          status: 400,
          headers: { "Cache-Control": "no-store" },
        });
      }

      const { channel_id, mode } = payload || {};

      // Validate channel_id: Discord snowflake (string of 15-22 digits)
      if (
        !channel_id ||
        (typeof channel_id !== "string" && typeof channel_id !== "number") ||
        !/^\d{15,22}$/.test(String(channel_id).trim())
      ) {
        return new Response("Missing or invalid channel_id", {
          status: 400,
          headers: { "Cache-Control": "no-store" },
        });
      }

      const cleanChannelId = String(channel_id).trim();

      // Validate mode: strictly 'terminal' or 'magpie'
      if (!mode || (mode !== "terminal" && mode !== "magpie")) {
        return new Response("Invalid mode", {
          status: 400,
          headers: { "Cache-Control": "no-store" },
        });
      }

      // Check mode store availability
      const kv = env.TERMINAL_KV || env.MAGPIE_KV;
      if (!kv) {
        return new Response("Mode store unavailable", {
          status: 503,
          headers: { "Cache-Control": "no-store" },
        });
      }

      try {
        await kv.put(`channel_mode:${cleanChannelId}`, mode, { expirationTtl: 3600 });
      } catch (putErr) {
        return new Response(`Failed to persist launch mode: ${putErr.message}`, {
          status: 500,
          headers: { "Cache-Control": "no-store" },
        });
      }

      // Note: We intentionally avoid setting a global mode fallback (`last_launch_mode`).
      // Cloudflare KV is eventually consistent. Writing a global fallback introduces a race condition
      // where two channels concurrently launching different activities cross-contaminate.
      // Each channel is strictly isolated by channel_id.

      return Response.json(
        {
          success: true,
          channel_id: cleanChannelId,
          mode,
        },
        {
          status: 200,
          headers: {
            "Content-Type": "application/json",
            "Cache-Control": "no-store",
          },
        }
      );
    }

    // 3. Security gate: Block all direct public internet access
    if (!isDiscordRequest(request)) {
      return new Response(
        "Access Denied: Direct internet access is blocked. Launch this activity directly from Discord (/magpie or /terminal).",
        {
          status: 403,
          headers: {
            "Content-Type": "text/plain",
            "Cache-Control": "no-store",
          },
        }
      );
    }

    // 4. Cryptographic Ed25519 verification if signature & public key are present
    const signature = request.headers.get("X-Signature-Ed25519");
    let rawBody = null;
    if (request.method !== "GET" && request.method !== "HEAD") {
      rawBody = await request.arrayBuffer();
    }

    if (signature && env.DISCORD_PUBLIC_KEY) {
      const isVerified = await verifyDiscordSignature(request, rawBody, env.DISCORD_PUBLIC_KEY);
      if (!isVerified) {
        return Response.json(
          { error: "Unauthorized: Invalid Discord cryptographic signature (Ed25519)" },
          {
            status: 401,
            headers: { "Cache-Control": "no-store" },
          }
        );
      }
    }

    // 5. Resolve active backend origin (path-based & launch-mode routing between Terminal and Magpie)
    let backendOrigin;
    let injectTerminalAuth = false;

    let pathname = url.pathname;
    if (pathname.startsWith("/.proxy/")) {
      pathname = "/" + pathname.slice(8);
    } else if (pathname === "/.proxy") {
      pathname = "/";
    }

    const channelId = url.searchParams.get("channel_id");
    const queryMode = url.searchParams.get("mode");
    const isWebSocket = request.headers.get("Upgrade")?.toLowerCase() === "websocket";

    // Terminal routing conditions:
    // - Path starts with /terminal (e.g. /terminal, /terminal/ws, /terminal/xterm.js)
    // - Path is /ws or /ws/ (Discord stripped prefix /terminal from URL mapping)
    // - Any WebSocket upgrade request (only the terminal bridge speaks WebSockets)
    let isTerminal = pathname.startsWith("/terminal") || pathname === "/ws" || pathname === "/ws/" || isWebSocket;

    if (!isTerminal && (pathname === "/" || pathname === "")) {
      if (queryMode === "terminal") {
        isTerminal = true;
      } else if (queryMode === "magpie") {
        isTerminal = false;
      } else {
        const kv = env.TERMINAL_KV || env.MAGPIE_KV;
        let mode = null;
        const cleanChannelId = channelId ? String(channelId).trim() : null;
        if (cleanChannelId && kv) {
          try {
            mode = await kv.get(`channel_mode:${cleanChannelId}`);
          } catch (_) {
            mode = null;
          }
        }
        if (mode === "terminal") {
          isTerminal = true;
        }
      }
    }

    if (isTerminal) {
      const terminalOrigin = (await env.TERMINAL_KV?.get("origin_url"))
        || (await env.MAGPIE_KV?.get("terminal_origin_url"))
        || env.TERMINAL_ORIGIN;
      if (!terminalOrigin) {
        return new Response("Terminal bridge origin is not configured yet.", {
          status: 503,
          headers: { "Content-Type": "text/plain", "Cache-Control": "no-store" },
        });
      }
      backendOrigin = terminalOrigin;
      injectTerminalAuth = true;
    } else {
      const targetOrigin = (await env.MAGPIE_KV?.get("origin_url")) || env.TARGET_ORIGIN;
      if (!targetOrigin) {
        return new Response("Magpie backend origin is not configured yet.", {
          status: 503,
          headers: { "Content-Type": "text/plain", "Cache-Control": "no-store" },
        });
      }
      backendOrigin = targetOrigin;
    }

    // Resolve active session authentication cookie (only for Magpie backend)
    const activeCookie = !injectTerminalAuth
      ? ((await env.MAGPIE_KV?.get("active_cookie")) || env.MAGPIE_COOKIE)
      : null;

    // 6. WebSocket proxying (streaming / terminal sessions)
    if (isWebSocket) {
      let wsPath = pathname;
      if (wsPath === "/ws" || wsPath === "/ws/") {
        wsPath = "/terminal/ws";
      } else if (wsPath === "/" || wsPath === "") {
        wsPath = "/terminal/ws";
      }

      const wsUrl = new URL(wsPath + url.search, backendOrigin);
      if (wsUrl.protocol === "wss:") wsUrl.protocol = "https:";
      if (wsUrl.protocol === "ws:") wsUrl.protocol = "http:";

      const wsHeaders = new Headers(request.headers);
      wsHeaders.set("Host", wsUrl.host);
      wsHeaders.set("Upgrade", "websocket");
      wsHeaders.set("Connection", "Upgrade");
      if (activeCookie) wsHeaders.set("Cookie", activeCookie);
      if (injectTerminalAuth && env.TERMINAL_SECRET) {
        wsHeaders.set("Authorization", `Bearer ${env.TERMINAL_SECRET}`);
      }

      try {
        const backendResponse = await fetch(wsUrl.toString(), {
          headers: wsHeaders,
        });

        const backendWs = backendResponse.webSocket;
        if (!backendWs) {
          return backendResponse;
        }

        const pair = new WebSocketPair();
        const [client, server] = Object.values(pair);

        server.binaryType = "arraybuffer";
        backendWs.binaryType = "arraybuffer";

        server.accept();
        backendWs.accept();

        server.addEventListener("message", async (event) => {
          try {
            let data = event.data;
            if (typeof data !== "string" && !(data instanceof ArrayBuffer) && !ArrayBuffer.isView(data)) {
              if (data && typeof data.arrayBuffer === "function") {
                data = await data.arrayBuffer();
              }
            }
            backendWs.send(data);
          } catch (_) {}
        });

        backendWs.addEventListener("message", async (event) => {
          try {
            let data = event.data;
            if (typeof data !== "string" && !(data instanceof ArrayBuffer) && !ArrayBuffer.isView(data)) {
              if (data && typeof data.arrayBuffer === "function") {
                data = await data.arrayBuffer();
              }
            }
            server.send(data);
          } catch (_) {}
        });

        server.addEventListener("close", (event) => {
          safeCloseWs(backendWs, event.code, event.reason);
        });

        backendWs.addEventListener("close", (event) => {
          safeCloseWs(server, event.code, event.reason);
        });

        server.addEventListener("error", () => {
          safeCloseWs(backendWs, 1011, "Client WS error");
        });

        backendWs.addEventListener("error", () => {
          safeCloseWs(server, 1011, "Backend WS error");
        });

        return new Response(null, {
          status: 101,
          webSocket: client,
        });
      } catch (err) {
        return new Response(`WebSocket connection failed: ${err.message}`, {
          status: 502,
          headers: { "Content-Type": "text/plain" },
        });
      }
    }

    // 7. Forward HTTP request with injected authentication
    let forwardPath = pathname;
    if (isTerminal && (forwardPath === "/" || forwardPath === "")) {
      forwardPath = "/terminal/";
    }
    const targetUrl = new URL(forwardPath + url.search, backendOrigin);

    const forwardHeaders = new Headers(request.headers);
    forwardHeaders.set("Host", targetUrl.host);
    if (activeCookie) forwardHeaders.set("Cookie", activeCookie);
    if (injectTerminalAuth && env.TERMINAL_SECRET) {
      forwardHeaders.set("Authorization", `Bearer ${env.TERMINAL_SECRET}`);
    }

    try {
      const resp = await fetch(targetUrl.toString(), {
        method: request.method,
        headers: forwardHeaders,
        body: rawBody,
        redirect: "manual",
      });

      // 8. Rewrite response headers for Discord iframe compatibility
      const respHeaders = new Headers(resp.headers);

      // Rewrite redirects back to Worker or preserve iframe host
      const location = respHeaders.get("Location");
      if (location) {
        try {
          const locUrl = new URL(location, backendOrigin);
          const fwdHost = request.headers.get("X-Forwarded-Host");
          if (fwdHost) {
            locUrl.host = fwdHost;
            locUrl.protocol = "https:";
            respHeaders.set("Location", locUrl.toString());
          } else {
            locUrl.host = url.host;
            locUrl.protocol = url.protocol;
            respHeaders.set("Location", locUrl.toString());
          }
        } catch (_) {}
      }

      // Remove headers that prevent iframe rendering inside Discord
      respHeaders.delete("X-Frame-Options");
      respHeaders.delete("Content-Security-Policy");

      // Enforce Cache-Control: no-store on Activity HTML and root entrypoints
      const contentType = (respHeaders.get("Content-Type") || "").toLowerCase();
      if (
        contentType.includes("text/html") ||
        contentType.includes("application/xhtml+xml") ||
        forwardPath === "/" ||
        forwardPath === "" ||
        forwardPath === "/terminal/" ||
        forwardPath === "/terminal"
      ) {
        respHeaders.set("Cache-Control", "no-store");
      }

      return new Response(resp.body, {
        status: resp.status,
        statusText: resp.statusText,
        headers: respHeaders,
      });
    } catch (err) {
      return new Response(`Failed to reach backend: ${err.message}`, {
        status: 502,
        headers: { "Content-Type": "text/plain", "Cache-Control": "no-store" },
      });
    }
  },
};
