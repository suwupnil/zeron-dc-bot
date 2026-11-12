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
  if (!rawPath) {
    return path.join(os.homedir(), '.ssh', 'id_ed25519');
  }
  if (rawPath.startsWith('~/')) {
    return path.join(os.homedir(), rawPath.slice(2));
  }
  return path.resolve(rawPath);
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

const HTML_PAGE = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no" />
  <title>Discord Activity VM Terminal</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.css" />
  <style>
    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }
    html, body {
      width: 100vw;
      height: 100vh;
      overflow: hidden;
      background-color: #1e1f22;
    }
    #terminal-container {
      width: 100vw;
      height: 100vh;
      padding: 4px;
      background-color: #000000;
    }
    .xterm {
      height: 100%;
    }
    .xterm-viewport {
      overflow-y: auto !important;
    }
  </style>
</head>
<body>
  <div id="terminal-container"></div>
  <a id="switch-link" href="/?mode=magpie" title="Switch to Magpie Web UI" style="position: absolute; top: 8px; right: 12px; z-index: 1000; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 11px; color: #949ba4; text-decoration: none; background: rgba(30, 31, 34, 0.85); padding: 4px 10px; border-radius: 4px; border: 1px solid #3f4147; backdrop-filter: blur(4px); transition: all 0.2s;" onmouseover="this.style.color='#f2f3f5';this.style.borderColor='#5865f2'" onmouseout="this.style.color='#949ba4';this.style.borderColor='#3f4147'">🐦 Magpie</a>
  <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.min.js"></script>
  <script>
    const switchLink = document.getElementById('switch-link');
    if (switchLink && location.search) {
      const cleanParams = new URLSearchParams(location.search);
      cleanParams.set('mode', 'magpie');
      switchLink.href = '/?' + cleanParams.toString();
    }
    const term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: 'Consolas, "Courier New", monospace, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto',
      theme: {
        background: '#000000',
        foreground: '#f0f0f0',
        cursor: '#5865f2'
      }
    });

    const fitAddon = new FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    term.open(document.getElementById('terminal-container'));
    fitAddon.fit();

    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = \`\${proto}//\${location.host}/terminal/ws\`;
    const ws = new WebSocket(wsUrl);
    ws.binaryType = 'arraybuffer';

    ws.onopen = () => {
      fitAddon.fit();
      ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
    };

    window.addEventListener('load', () => {
      fitAddon.fit();
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      }
    });

    window.addEventListener('resize', () => {
      fitAddon.fit();
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      }
    });

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(data);
      }
    });

    ws.onmessage = (event) => {
      if (typeof event.data === 'string') {
        term.write(event.data);
      } else {
        term.write(new Uint8Array(event.data));
      }
    };

    ws.onclose = () => {
      term.write('\\r\\n[disconnected]\\r\\n');
    };

    ws.onerror = () => {
      term.write('\\r\\n[connection error]\\r\\n');
    };
  </script>
</body>
</html>`;

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host || '127.0.0.1'}`);

  if ((req.method === 'GET' || req.method === 'HEAD') && (url.pathname === '/terminal/' || url.pathname === '/terminal' || url.pathname === '/' || url.pathname === '')) {
    res.writeHead(200, {
      'Content-Type': 'text/html; charset=utf-8',
      'Cache-Control': 'no-cache, no-store, must-revalidate',
    });
    res.end(HTML_PAGE);
    return;
  }

  // Handle /terminal/ws non-upgrade HTTP requests
  if (url.pathname === '/terminal/ws' || url.pathname === '/terminal/ws/') {
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

const wss = new WebSocketServer({ noServer: true });
const activeSockets = new Set();

server.on('upgrade', (req, socket, head) => {
  const url = new URL(req.url, `http://${req.headers.host || '127.0.0.1'}`);

  if (url.pathname !== '/terminal/ws' && url.pathname !== '/terminal/ws/') {
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
          cleanup();
        });

        // Drain any pending keystrokes / data buffered during SSH handshake
        while (pendingBuffer.length > 0) {
          const item = pendingBuffer.shift();
          if (sshStream.writable) {
            sshStream.write(item);
          }
        }

        // Pipe SSH stdout to WebSocket
        stream.on('data', (data) => {
          if (ws.readyState === ws.OPEN) {
            ws.send(data);
          }
        });

        if (stream.stderr) {
          stream.stderr.on('error', () => {});
          stream.stderr.on('data', (data) => {
            if (ws.readyState === ws.OPEN) {
              ws.send(data);
            }
          });
        }

        stream.on('close', () => {
          cleanup();
        });
      }

      // Try executing $SHELL or fallback bash with PTY
      sshClient.exec(shellCmd, {
        pty: {
          term: 'xterm-256color',
          rows: initialRows,
          cols: initialCols,
        },
      }, (err, stream) => {
        if (err) {
          // If exec is rejected by remote sshd configuration, fallback to client.shell
          sshClient.shell({
            term: 'xterm-256color',
            rows: initialRows,
            cols: initialCols,
          }, onStreamReady);
          return;
        }
        onStreamReady(null, stream);
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

  ws.on('message', (data) => {
    if (!firstMessageReceived) {
      firstMessageReceived = true;
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
