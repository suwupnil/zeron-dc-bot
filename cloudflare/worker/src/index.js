/**
 * Cloudflare Worker: Discord Embedded Activity Reverse Proxy (Magpie Web UI)
 *
 * Provides a permanent, high-performance HTTPS entrypoint (*.workers.dev) for
 * Discord Activities, proxying traffic to a local or tunneled Magpie backend.
 *
 * Capabilities:
 * - Cryptographic Discord Proxy Verification (Ed25519 via native Web Crypto API).
 * - Self-healing dynamic backend origin resolution backed by Workers KV.
 * - Auto-authentication injection (magpie_web_4000 session cookie).
 * - Discord iframe security header normalization (strips X-Frame-Options & CSP).
 * - Full HTTP streaming and WebSocket upgrade proxying.
 */

// Convert hex string to Uint8Array
function hexToUint8Array(hex) {
  if (!hex || typeof hex !== "string") return new Uint8Array(0);
  const cleanHex = hex.trim();
  const matches = cleanHex.match(/.{1,2}/g);
  if (!matches) return new Uint8Array(0);
  return new Uint8Array(matches.map((byte) => parseInt(byte, 16)));
}

// Cryptographic Ed25519 verification using native Web Crypto API (crypto.subtle)
async function verifyDiscordSignature(request, rawBodyBuffer, publicKeyHex) {
  const signature = request.headers.get("X-Signature-Ed25519");
  const timestamp = request.headers.get("X-Signature-Timestamp");

  if (!signature || !timestamp || !publicKeyHex) {
    return false;
  }

  try {
    const keyBytes = hexToUint8Array(publicKeyHex);
    const sigBytes = hexToUint8Array(signature);

    if (keyBytes.length !== 32 || sigBytes.length !== 64) {
      console.warn("Invalid key (must be 32 bytes) or signature (must be 64 bytes) for Ed25519");
      return false;
    }

    const cryptoKey = await crypto.subtle.importKey(
      "raw",
      keyBytes,
      { name: "Ed25519" },
      false,
      ["verify"]
    );

    const encoder = new TextEncoder();
    const timestampBytes = encoder.encode(timestamp);
    const bodyBytes = rawBodyBuffer ? new Uint8Array(rawBodyBuffer) : new Uint8Array(0);

    const merged = new Uint8Array(timestampBytes.length + bodyBytes.length);
    merged.set(timestampBytes, 0);
    merged.set(bodyBytes, timestampBytes.length);

    return await crypto.subtle.verify(
      { name: "Ed25519" },
      cryptoKey,
      sigBytes,
      merged
    );
  } catch (err) {
    console.error("Cryptographic verification error:", err);
    return false;
  }
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);

    // 1. Health check & status endpoint (GET /__status)
    if (url.pathname === "/__status") {
      const activeOrigin = (await env.MAGPIE_KV?.get("origin_url")) || env.TARGET_ORIGIN;
      return new Response(
        JSON.stringify({
          status: "healthy",
          worker: "magpie-activity",
          active_backend_origin: activeOrigin,
          ed25519_verification_enforced: Boolean(env.DISCORD_PUBLIC_KEY),
          timestamp: new Date().toISOString(),
        }),
        { headers: { "Content-Type": "application/json" } }
      );
    }

    // 2. Secret self-healing update endpoint from VM (POST /__update_origin)
    if (url.pathname === "/__update_origin") {
      if (request.method !== "POST") {
        return new Response("Method Not Allowed", { status: 405 });
      }

      const auth = request.headers.get("Authorization");
      if (!env.UPDATE_SECRET || auth !== `Bearer ${env.UPDATE_SECRET}`) {
        return new Response("Unauthorized", { status: 401 });
      }

      try {
        const body = await request.json();
        if (!body.origin || !body.origin.startsWith("http")) {
          return new Response("Invalid origin URL", { status: 400 });
        }

        if (env.MAGPIE_KV) {
          await env.MAGPIE_KV.put("origin_url", body.origin);
        }

        return new Response(
          JSON.stringify({ success: true, updated_origin: body.origin }),
          { headers: { "Content-Type": "application/json" } }
        );
      } catch (e) {
        return new Response(`Bad request: ${e.message}`, { status: 400 });
      }
    }

    // 3. Cryptographic Discord Proxy Verification (Strictest Mode)
    // When DISCORD_PUBLIC_KEY is configured, only Discord's edge proxy can communicate with the backend.
    let rawBody = null;
    if (request.method !== "GET" && request.method !== "HEAD") {
      rawBody = await request.arrayBuffer();
    }

    if (env.DISCORD_PUBLIC_KEY) {
      const isVerified = await verifyDiscordSignature(request, rawBody, env.DISCORD_PUBLIC_KEY);
      if (!isVerified) {
        return new Response(
          JSON.stringify({
            error: "Unauthorized: Invalid or missing Discord cryptographic signature (Ed25519)",
            hint: "Requests must originate from Discord's embedded activity proxy with valid X-Signature-Ed25519 headers.",
          }),
          { status: 401, headers: { "Content-Type": "application/json" } }
        );
      }
    }

    // 4. Resolve active backend origin from KV or fallback
    let targetOrigin = (await env.MAGPIE_KV?.get("origin_url")) || env.TARGET_ORIGIN;
    if (!targetOrigin) {
      return new Response(
        "Magpie backend origin is not configured yet.",
        { status: 503, headers: { "Content-Type": "text/plain" } }
      );
    }

    const targetUrl = new URL(url.pathname + url.search, targetOrigin);

    // 5. Handle WebSockets (if Magpie terminal/streaming uses WS)
    const upgradeHeader = request.headers.get("Upgrade");
    if (upgradeHeader && upgradeHeader.toLowerCase() === "websocket") {
      const wsUrl = new URL(url.pathname + url.search, targetOrigin);
      wsUrl.protocol = wsUrl.protocol === "https:" ? "wss:" : "ws:";

      const newWsHeaders = new Headers(request.headers);
      newWsHeaders.set("Host", wsUrl.host);
      if (env.MAGPIE_COOKIE) {
        newWsHeaders.set("Cookie", env.MAGPIE_COOKIE);
      }

      return fetch(wsUrl.toString(), {
        headers: newWsHeaders,
      });
    }

    // 6. Build forwarded HTTP request
    const forwardHeaders = new Headers(request.headers);
    forwardHeaders.set("Host", targetUrl.host);
    // Inject Magpie session authentication cookie if configured
    if (env.MAGPIE_COOKIE) {
      forwardHeaders.set("Cookie", env.MAGPIE_COOKIE);
    }

    try {
      const resp = await fetch(targetUrl.toString(), {
        method: request.method,
        headers: forwardHeaders,
        body: rawBody,
        redirect: "manual",
      });

      // 7. Rewrite response headers for Discord iframe compatibility
      const respHeaders = new Headers(resp.headers);

      // Rewrite 301/302/303 redirects back to the Worker host
      const location = respHeaders.get("Location");
      if (location) {
        try {
          const locUrl = new URL(location, targetOrigin);
          locUrl.host = url.host;
          locUrl.protocol = url.protocol;
          respHeaders.set("Location", locUrl.toString());
        } catch (_) {}
      }

      // Remove headers that prevent iframe rendering inside Discord
      respHeaders.delete("X-Frame-Options");
      respHeaders.delete("Content-Security-Policy");

      if (env.DISCORD_PUBLIC_KEY) {
        respHeaders.set("X-Discord-Verify", "ed25519-enforced");
      }

      return new Response(resp.body, {
        status: resp.status,
        statusText: resp.statusText,
        headers: respHeaders,
      });
    } catch (err) {
      return new Response(
        `Failed to reach Magpie backend: ${err.message}`,
        { status: 502, headers: { "Content-Type": "text/plain" } }
      );
    }
  },
};
