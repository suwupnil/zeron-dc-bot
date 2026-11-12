#!/usr/bin/env node
/**
 * terminal-bridge.js
 *
 * Lightweight Node.js WebSocket-to-SSH proxy bridge for Discord Activity VM Terminal.
 * Listens on 127.0.0.1:7681 only.
 */

'use strict';

const http = require('http');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { WebSocketServer } = require('ws');
const { Client } = require('ssh2');

const PORT = parseInt(process.env.TERMINAL_PORT || '7681', 10) || 7681;
const HOST = '127.0.0.1';
const TERMINAL_SECRET = process.env.TERMINAL_SECRET;
const SSH_HOST = process.env.TERMINAL_SSH_HOST || '127.0.0.1';
const SSH_PORT = parseInt(process.env.TERMINAL_SSH_PORT || '22', 10) || 22;

function safeCompare(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string') return false;
  const bufA = Buffer.from(a);
  const bufB = Buffer.from(b);
  if (bufA.length !== bufB.length) return false;
  return crypto.timingSafeEqual(bufA, bufB);
}

function isAuthorized(authHeader) {
  if (!TERMINAL_SECRET || !authHeader) return false;
  return safeCompare(authHeader, `Bearer ${TERMINAL_SECRET}`);
}

function resolveKeyPath(rawPath) {
  if (rawPath) {
    if (rawPath.startsWith('~/')) {
      return path.join(os.homedir(), rawPath.slice(2));
    }
    return path.resolve(rawPath);
  }
  const candidates = [
    path.join(os.homedir(), '.ssh', 'id_ed25519'),
    path.join(os.homedir(), '.ssh', 'id_rsa'),
    path.join(os.homedir(), '.ssh', 'id_ecdsa'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return candidates[0];
}

function getSSHUser() {
  if (process.env.TERMINAL_SSH_USER) {
    return process.env.TERMINAL_SSH_USER;
  }
  try {
    return os.userInfo().username;
  } catch (_) {
    return process.env.USER || process.env.USERNAME || 'root';
  }
}

function readAsset(filename) {
  const candidates = [
    path.join(__dirname, 'vendor', filename),
    path.join(__dirname, 'node_modules', filename),
    path.join(__dirname, filename),
    path.join('/home/suwu/node_modules', filename),
    path.join(os.homedir(), 'node_modules', filename),
  ];
  for (const c of candidates) {
    try {
      if (fs.existsSync(c)) {
        return fs.readFileSync(c);
      }
    } catch (_) {}
  }
  return null;
}

const XTERM_JS = readAsset('xterm.js') || readAsset('xterm/lib/xterm.js');
const XTERM_CSS = readAsset('xterm.css') || readAsset('xterm/css/xterm.css');
const ADDON_FIT_JS = readAsset('addon-fit.js') || readAsset('@xterm/addon-fit/lib/addon-fit.js');

function buildHtmlPage() {
  const css = XTERM_CSS ? XTERM_CSS.toString('utf8') : '';
  const xtermJs = XTERM_JS ? XTERM_JS.toString('utf8') : '';
  const fitJs = ADDON_FIT_JS ? ADDON_FIT_JS.toString('utf8') : '';

  return [
    '<!DOCTYPE html>',
    '<html lang="en">',
    '<head>',
    '  <meta charset="utf-8" />',
    '  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no" />',
    '  <title>VM Terminal</title>',
    '  <style>',
    css,
    '    * { box-sizing: border-box; margin: 0; padding: 0; }',
    '    html, body { width: 100vw; height: 100vh; overflow: hidden; background-color: #000000; }',
    '    #terminal-container { width: 100vw; height: 100vh; padding: 4px; background-color: #000000; }',
    '    .xterm { height: 100%; }',
    '    .xterm-viewport { overflow-y: auto !important; }',
    '  </style>',
    '</head>',
    '<body>',
    '  <div id="terminal-container"></div>',
    '  <script>',
    xtermJs,
    '  </script>',
    '  <script>',
    fitJs,
    '  </script>',
    '  <script>',
    `
    const TerminalConstructor = (window.Terminal && window.Terminal.Terminal) || window.Terminal;
    const FitAddonConstructor = (window.FitAddon && window.FitAddon.FitAddon) || window.FitAddon;

    if (!TerminalConstructor) {
      document.body.innerHTML = '<div style="color:#ff6b6b;padding:24px;font-family:monospace;font-size:14px;background:#18191c;margin:20px;border-radius:6px;border:1px solid #ed4245;">Failed to load xterm.js bundle.</div>';
      throw new Error('Terminal constructor not found');
    }

    const term = new TerminalConstructor({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: 'Consolas, "Courier New", monospace, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto',
      theme: {
        background: '#000000',
        foreground: '#f0f0f0',
        cursor: '#5865f2'
      }
    });

    const fitAddon = FitAddonConstructor ? new FitAddonConstructor() : null;
    if (fitAddon) {
      term.loadAddon(fitAddon);
    }
    const container = document.getElementById('terminal-container');
    term.open(container);
    if (fitAddon) {
      try { fitAddon.fit(); } catch (_) {}
    }
    term.focus();

    window.addEventListener('click', () => term.focus());
    container.addEventListener('touchstart', () => term.focus(), { passive: true });

    term.write('\\x1b[90m[connecting to terminal session...]\\x1b[0m\\r\\n');

    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = location.host;
    const search = location.search || '';
    const isDiscord = location.hostname.includes('discordsays.com');

    // Build comprehensive candidate list to handle both Discord URL mapping topologies:
    // Case 1: Prefix "/" mapped to worker -> /.proxy/terminal/ws and /.proxy/ws forwarded intact.
    // Case 2: Prefix "/terminal" mapped to worker -> Discord strips prefix, mapping to /ws.
    // Also tests both with and without Discord's launch query parameters.
    const candidates = [];
    if (isDiscord) {
      candidates.push(proto + '//' + host + '/.proxy/terminal/ws' + search);
      candidates.push(proto + '//' + host + '/.proxy/terminal/ws');
      candidates.push(proto + '//' + host + '/.proxy/ws' + search);
      candidates.push(proto + '//' + host + '/.proxy/ws');
      candidates.push(proto + '//' + host + '/terminal/ws' + search);
      candidates.push(proto + '//' + host + '/terminal/ws');
      candidates.push(proto + '//' + host + '/ws' + search);
      candidates.push(proto + '//' + host + '/ws');
    } else {
      candidates.push(proto + '//' + host + '/terminal/ws' + search);
      candidates.push(proto + '//' + host + '/terminal/ws');
      candidates.push(proto + '//' + host + '/ws' + search);
      candidates.push(proto + '//' + host + '/ws');
      candidates.push(proto + '//' + host + '/.proxy/terminal/ws' + search);
      candidates.push(proto + '//' + host + '/.proxy/terminal/ws');
      candidates.push(proto + '//' + host + '/.proxy/ws');
    }

    let activeWs = null;
    let isConnected = false;
    let endpointIndex = 0;
    let attemptTimer = null;

    function tryConnect() {
      if (isConnected) return;
      if (attemptTimer) {
        clearTimeout(attemptTimer);
        attemptTimer = null;
      }
      if (endpointIndex >= candidates.length) {
        term.write('\\r\\n\\x1b[31m[Failed to connect to terminal bridge after ' + candidates.length + ' attempts. Please re-run /terminal in Discord.]\\x1b[0m\\r\\n');
        return;
      }

      const attemptNum = endpointIndex + 1;
      const target = candidates[endpointIndex++];
      let displayPath = target;
      try {
        const u = new URL(target);
        displayPath = u.pathname + (u.search ? u.search.slice(0, 32) + (u.search.length > 32 ? '...' : '') : '');
      } catch (_) {}

      term.write('\\x1b[90m[Attempt ' + attemptNum + '/' + candidates.length + ': connecting to ' + displayPath + '...]\\x1b[0m\\r\\n');

      let ws;
      try {
        ws = new WebSocket(target);
      } catch (err) {
        term.write('\\x1b[31m[Attempt ' + attemptNum + ' error: ' + (err.message || 'Constructor error') + ']\\x1b[0m\\r\\n');
        attemptTimer = setTimeout(tryConnect, 150);
        return;
      }

      ws.binaryType = 'arraybuffer';
      activeWs = ws;

      function sendResize() {
        if (fitAddon) {
          try { fitAddon.fit(); } catch (_) {}
        }
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
        }
      }

      let connectionHandled = false;

      ws.onopen = () => {
        if (connectionHandled) return;
        connectionHandled = true;
        isConnected = true;
        term.write('\\x1b[2K\\r\\x1b[32m[Connected to terminal bridge via ' + displayPath + ']\\x1b[0m\\r\\n');
        sendResize();
        setTimeout(sendResize, 150);
      };

      ws.onmessage = (event) => {
        if (typeof event.data === 'string') {
          term.write(event.data);
        } else {
          term.write(new Uint8Array(event.data));
        }
      };

      ws.onerror = (ev) => {
        console.warn('[Terminal WS] Error on ' + target, ev);
        // Do not call tryConnect() here because onclose will fire immediately after onerror!
      };

      ws.onclose = (ev) => {
        if (isConnected) {
          term.write('\\r\\n\\x1b[33m[Session closed (code ' + ev.code + (ev.reason ? ': ' + ev.reason : '') + ')]\\x1b[0m\\r\\n');
        } else {
          if (connectionHandled) return;
          connectionHandled = true;
          const reasonStr = ev.reason ? ' (' + ev.reason + ')' : '';
          const codeDetail = ev.code ? 'code ' + ev.code : 'unknown code';
          term.write('\\x1b[33m[Attempt ' + attemptNum + ' failed: ' + codeDetail + reasonStr + ']\\x1b[0m\\r\\n');
          attemptTimer = setTimeout(tryConnect, 150);
        }
      };

      window.addEventListener('resize', sendResize);
    }

    term.onData((data) => {
      if (activeWs && activeWs.readyState === WebSocket.OPEN) {
        activeWs.send(data);
      }
    });

    tryConnect();
    `,
    '  </script>',
    '</body>',
    '</html>',
  ].join('\n');
}

const HTML_PAGE = buildHtmlPage();

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host || '127.0.0.1'}`);
  let pathname = url.pathname;
  if (pathname.startsWith('/.proxy/')) {
    pathname = '/' + pathname.slice(8);
  } else if (pathname === '/.proxy') {
    pathname = '/';
  }

  console.log(`[terminal-bridge] ${req.method} ${pathname}`);

  if (req.method === 'GET' || req.method === 'HEAD') {
    if (pathname === '/terminal/xterm.js' || pathname === '/xterm.js') {
      if (XTERM_JS) {
        res.writeHead(200, {
          'Content-Type': 'application/javascript; charset=utf-8',
          'Cache-Control': 'public, max-age=86400',
        });
        res.end(XTERM_JS);
      } else {
        res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('xterm.js not found');
      }
      return;
    }

    if (pathname === '/terminal/xterm.css' || pathname === '/xterm.css') {
      if (XTERM_CSS) {
        res.writeHead(200, {
          'Content-Type': 'text/css; charset=utf-8',
          'Cache-Control': 'public, max-age=86400',
        });
        res.end(XTERM_CSS);
      } else {
        res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('xterm.css not found');
      }
      return;
    }

    if (pathname === '/terminal/addon-fit.js' || pathname === '/addon-fit.js') {
      if (ADDON_FIT_JS) {
        res.writeHead(200, {
          'Content-Type': 'application/javascript; charset=utf-8',
          'Cache-Control': 'public, max-age=86400',
        });
        res.end(ADDON_FIT_JS);
      } else {
        res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('addon-fit.js not found');
      }
      return;
    }

    if (
      pathname === '/terminal/' ||
      pathname === '/terminal' ||
      pathname === '/terminal.html' ||
      pathname === '/' ||
      pathname === '/index.html' ||
      pathname === ''
    ) {
      res.writeHead(200, {
        'Content-Type': 'text/html; charset=utf-8',
        'Cache-Control': 'no-cache, no-store, must-revalidate',
      });
      res.end(HTML_PAGE);
      return;
    }
  }

  // Handle /terminal/ws or /ws non-upgrade HTTP requests
  if (
    pathname === '/terminal/ws' ||
    pathname === '/terminal/ws/' ||
    pathname === '/ws' ||
    pathname === '/ws/'
  ) {
    const authHeader = req.headers['authorization'];
    if (!isAuthorized(authHeader)) {
      res.writeHead(401, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end('Unauthorized');
      return;
    }
    // Authorized but not a WebSocket upgrade request
    res.writeHead(426, {
      'Content-Type': 'text/plain; charset=utf-8',
      'Upgrade': 'WebSocket',
      'Connection': 'Upgrade',
    });
    res.end('Upgrade Required');
    return;
  }

  res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
  res.end('Not Found');
});

const VALID_WS_PATHS = new Set(['/terminal/ws', '/terminal/ws/', '/ws', '/ws/']);
const wss = new WebSocketServer({ noServer: true });
const activeSockets = new Set();

server.on('upgrade', (req, socket, head) => {
  const url = new URL(req.url, `http://${req.headers.host || '127.0.0.1'}`);
  let pathname = url.pathname;
  if (pathname.startsWith('/.proxy/')) {
    pathname = '/' + pathname.slice(8);
  }

  console.log(`[terminal-bridge] UPGRADE ${pathname} auth=${Boolean(req.headers['authorization'])}`);

  if (!VALID_WS_PATHS.has(pathname)) {
    console.warn(`[terminal-bridge] UPGRADE rejected invalid path: ${pathname}`);
    socket.write('HTTP/1.1 404 Not Found\r\n\r\n');
    socket.destroy();
    return;
  }

  wss.handleUpgrade(req, socket, head, (ws) => {
    wss.emit('connection', ws, req);
  });
});

wss.on('connection', (ws, req) => {
  activeSockets.add(ws);

  // Validate Authorization header
  const authHeader = req.headers['authorization'];
  if (!isAuthorized(authHeader)) {
    console.warn('[terminal-bridge] Unauthorized WebSocket upgrade rejected');
    ws.close(1008, 'Unauthorized');
    activeSockets.delete(ws);
    return;
  }

  console.log('[terminal-bridge] Authorized WebSocket connected. Spawning SSH...');

  let sshClient = null;
  let sshStream = null;
  let isClosed = false;
  let firstMessageReceived = false;
  const pendingBuffer = [];

  function cleanup() {
    if (isClosed) return;
    isClosed = true;

    activeSockets.delete(ws);

    if (sshStream) {
      try { sshStream.end(); } catch (_) {}
      try { sshStream.close(); } catch (_) {}
      sshStream = null;
    }

    if (sshClient) {
      try { sshClient.end(); } catch (_) {}
      try { sshClient.destroy(); } catch (_) {}
      sshClient = null;
    }

    if (ws.readyState === ws.OPEN || ws.readyState === ws.CONNECTING) {
      try { ws.close(); } catch (_) {}
    }
  }

  function startSSH(initialCols, initialRows) {
    const keyPath = resolveKeyPath(process.env.TERMINAL_SSH_KEY);
    const user = getSSHUser();

    let privateKey;
    try {
      privateKey = fs.readFileSync(keyPath);
    } catch (err) {
      console.error(`[terminal-bridge] Failed to read private key from ${keyPath}:`, err.message);
      if (ws.readyState === ws.OPEN) {
        ws.send(`\r\n\x1b[31m[SSH Bridge Error: Failed to read key from ${keyPath}]\x1b[0m\r\n`, () => {
          try { ws.close(1011, 'Missing SSH key'); } catch (_) {}
          cleanup();
        });
      } else {
        cleanup();
      }
      return;
    }

    sshClient = new Client();

    sshClient.on('error', (err) => {
      console.error('[terminal-bridge] SSH client error:', err.message);
      if (ws.readyState === ws.OPEN) {
        ws.send(`\r\n\x1b[31m[SSH Error: ${err.message}]\x1b[0m\r\n`);
      }
      cleanup();
    });

    sshClient.on('close', () => {
      cleanup();
    });

    sshClient.on('end', () => {
      cleanup();
    });

    sshClient.on('ready', () => {
      let shellCmd = process.env.SHELL;
      if (!shellCmd) {
        try {
          shellCmd = os.userInfo().shell;
        } catch (_) {}
      }
      if (!shellCmd) {
        shellCmd = 'bash';
      }

      function onStreamReady(err, stream) {
        if (err) {
          console.error('[terminal-bridge] Failed to allocate PTY / shell:', err.message);
          cleanup();
          return;
        }

        sshStream = stream;

        // Prevent unhandled stream error events from terminating the process
        stream.on('error', (streamErr) => {
          console.error('[terminal-bridge] SSH stream error:', streamErr.message);
        });

        stream.on('close', () => {
          cleanup();
        });

        // Pipe SSH stdout/stderr directly to WebSocket client
        stream.on('data', (chunk) => {
          if (ws.readyState === ws.OPEN) {
            try {
              ws.send(chunk);
            } catch (_) {}
          }
        });

        if (stream.stderr) {
          stream.stderr.on('data', (chunk) => {
            if (ws.readyState === ws.OPEN) {
              try {
                ws.send(chunk);
              } catch (_) {}
            }
          });
        }

        // Flush any keystrokes sent before stream was fully ready
        if (pendingBuffer.length > 0) {
          for (const buf of pendingBuffer) {
            try {
              stream.write(buf);
            } catch (_) {}
          }
          pendingBuffer.length = 0;
        }
      }

      const ptyOptions = {
        term: 'xterm-256color',
        cols: initialCols || 80,
        rows: initialRows || 24,
      };

      sshClient.shell(ptyOptions, (shellErr, stream) => {
        if (!shellErr && stream) {
          onStreamReady(null, stream);
          return;
        }

        console.warn('[terminal-bridge] sshClient.shell() failed, falling back to pty exec:', shellErr ? shellErr.message : 'no stream');
        sshClient.exec(shellCmd, { pty: ptyOptions }, (execErr, fallbackStream) => {
          if (execErr) {
            onStreamReady(execErr, null);
            return;
          }
          onStreamReady(null, fallbackStream);
        });
      });
    });

    try {
      sshClient.connect({
        host: SSH_HOST,
        port: SSH_PORT,
        username: user,
        privateKey,
        passphrase: process.env.TERMINAL_SSH_PASSPHRASE,
        readyTimeout: 10000,
      });
    } catch (err) {
      console.error('[terminal-bridge] Failed to initialize SSH connection:', err.message);
      if (ws.readyState === ws.OPEN) {
        ws.send(`\r\n\x1b[31m[SSH Connection Error: ${err.message}]\x1b[0m\r\n`, () => {
          try { ws.close(1011, 'SSH Init Failed'); } catch (_) {}
          cleanup();
        });
      } else {
        cleanup();
      }
    }
  }

  // Fallback timer: start SSH even if resize message is delayed
  const initTimer = setTimeout(() => {
    if (!firstMessageReceived && !isClosed) {
      firstMessageReceived = true;
      console.log('[terminal-bridge] Starting SSH with default dimensions (80x24)...');
      startSSH(80, 24);
    }
  }, 400);

  ws.on('message', (data) => {
    if (!firstMessageReceived) {
      firstMessageReceived = true;
      clearTimeout(initTimer);
      let cols = 80;
      let rows = 24;

      try {
        const text = data.toString('utf8');
        const parsed = JSON.parse(text);
        if (parsed.type === 'resize') {
          if (typeof parsed.cols === 'number' && parsed.cols > 0 && parsed.cols < 1000) {
            cols = Math.floor(parsed.cols);
          }
          if (typeof parsed.rows === 'number' && parsed.rows > 0 && parsed.rows < 1000) {
            rows = Math.floor(parsed.rows);
          }
        } else if (pendingBuffer.length < 1024) {
          pendingBuffer.push(data);
        }
      } catch (_) {
        if (pendingBuffer.length < 1024) {
          pendingBuffer.push(data);
        }
      }

      console.log(`[terminal-bridge] Starting SSH with dimensions (${cols}x${rows})...`);
      startSSH(cols, rows);
      return;
    }

    // Check if subsequent message is a resize control frame
    try {
      const text = data.toString('utf8');
      if (text.trim().startsWith('{')) {
        const parsed = JSON.parse(text);
        if (parsed.type === 'resize' && typeof parsed.cols === 'number' && typeof parsed.rows === 'number') {
          const newCols = Math.max(1, Math.min(1000, Math.floor(parsed.cols)));
          const newRows = Math.max(1, Math.min(1000, Math.floor(parsed.rows)));
          if (sshStream) {
            sshStream.setWindow(newRows, newCols, 0, 0);
          }
          return;
        }
      }
    } catch (_) {}

    // Forward raw binary / text data: WS -> SSH stdin
    if (sshStream && sshStream.writable) {
      sshStream.write(data);
    } else if (pendingBuffer.length < 1024) {
      pendingBuffer.push(data);
    }
  });

  ws.on('close', () => {
    cleanup();
  });

  ws.on('error', (err) => {
    console.error('[terminal-bridge] WebSocket client error:', err.message);
    cleanup();
  });
});

server.on('error', (err) => {
  if (err.code === 'EADDRINUSE') {
    console.error(`[terminal-bridge] Port ${PORT} is already in use.`);
  } else {
    console.error('[terminal-bridge] Server error:', err.message);
  }
  process.exit(1);
});

server.listen(PORT, HOST, () => {
  console.log(`[terminal-bridge] Listening on http://${HOST}:${PORT}`);
});

function gracefulShutdown() {
  console.log('[terminal-bridge] Received termination signal. Closing bridge...');
  for (const ws of activeSockets) {
    try { ws.close(1001, 'Server shutting down'); } catch (_) {}
  }
  server.close(() => {
    console.log('[terminal-bridge] Server stopped.');
    process.exit(0);
  });
  setTimeout(() => {
    process.exit(0);
  }, 3000).unref();
}

process.on('SIGTERM', gracefulShutdown);
process.on('SIGINT', gracefulShutdown);
