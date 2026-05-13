pub const GUI_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Model Router</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #101114;
      --panel: #171a20;
      --line: #2b313c;
      --text: #eef2f7;
      --muted: #9aa4b2;
      --accent: #66d9c6;
      --warn: #f4bf75;
      --bad: #ff7b72;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font: 14px/1.45 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    main { width: min(1180px, calc(100vw - 32px)); margin: 24px auto; }
    header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px; }
    h1 { margin: 0; font-size: 24px; letter-spacing: 0; }
    .grid { display: grid; grid-template-columns: minmax(0, 1fr) 360px; gap: 14px; align-items: start; }
    .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 14px;
    }
    label { display: block; color: var(--muted); margin-bottom: 6px; }
    textarea, select, input {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #0d0f13;
      color: var(--text);
      padding: 10px;
      font: inherit;
    }
    textarea { min-height: 180px; resize: vertical; }
    .row { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; margin: 12px 0; }
    .actions { display: flex; gap: 8px; flex-wrap: wrap; }
    button {
      border: 1px solid var(--line);
      border-radius: 6px;
      color: var(--text);
      background: #202632;
      padding: 9px 12px;
      font: inherit;
      cursor: pointer;
    }
    button.primary { background: #14544d; border-color: #218576; }
    button:hover { border-color: var(--accent); }
    pre {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      background: #0d0f13;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 12px;
      min-height: 180px;
    }
    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; border-bottom: 1px solid var(--line); padding: 7px 4px; vertical-align: top; }
    th { color: var(--muted); font-weight: 600; }
    code {
      display: block;
      overflow-wrap: anywhere;
      color: var(--accent);
      background: #0d0f13;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 7px 8px;
      margin: 6px 0;
    }
    .available { color: var(--accent); }
    .unavailable { color: var(--bad); }
    .disabled { color: var(--warn); }
    @media (max-width: 860px) {
      main { width: min(100vw - 20px, 720px); margin-top: 14px; }
      .grid { grid-template-columns: 1fr; }
      .row { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <main>
    <header>
      <h1>Model Router</h1>
      <button id="refreshHealth" type="button">Refresh health</button>
    </header>
    <div class="grid">
      <section class="panel">
        <label for="prompt">Prompt</label>
        <textarea id="prompt">Fix the failing parser tests in this repo.</textarea>
        <div class="row">
          <div>
            <label for="prefer">Provider</label>
            <select id="prefer">
              <option value="">auto</option>
              <option value="local">local</option>
              <option value="codex">codex</option>
              <option value="claude">claude</option>
              <option value="gemini">gemini</option>
              <option value="lmstudio">lmstudio</option>
              <option value="llamacpp">llamacpp</option>
            </select>
          </div>
          <div>
            <label for="hint">Hint</label>
            <select id="hint">
              <option value="">auto</option>
              <option value="simple">simple</option>
              <option value="code">code</option>
              <option value="deep_reasoning">deep reasoning</option>
              <option value="writing">writing</option>
            </select>
          </div>
          <div>
            <label for="cwd">Working directory</label>
            <input id="cwd" placeholder="/path/to/project">
          </div>
        </div>
        <div class="actions">
          <button class="primary" id="routeBtn" type="button">Route</button>
          <button id="runBtn" type="button">Run</button>
          <button id="queueBtn" type="button">Queue</button>
        </div>
        <pre id="output"></pre>
      </section>
      <aside class="panel">
        <h2>Setup</h2>
        <code>modelrouter init</code>
        <code>modelrouter doctor</code>
        <code>modelrouter daemon start</code>
        <h2>Metrics</h2>
        <pre id="metrics">No request log loaded.</pre>
        <h2>Queue</h2>
        <pre id="queue">No queued jobs yet.</pre>
        <table>
          <thead><tr><th>Provider</th><th>Status</th></tr></thead>
          <tbody id="health"></tbody>
        </table>
      </aside>
    </div>
  </main>
  <script>
    const output = document.getElementById('output');
    const health = document.getElementById('health');
    function payload() {
      const body = { prompt: document.getElementById('prompt').value };
      for (const id of ['prefer', 'hint', 'cwd']) {
        const value = document.getElementById(id).value.trim();
        if (value) body[id] = value;
      }
      return body;
    }
    async function post(path) {
      output.textContent = 'Working...';
      const res = await fetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload())
      });
      const json = await res.json();
      output.textContent = JSON.stringify(json, null, 2);
    }
    async function loadHealth() {
      const res = await fetch('/health');
      const json = await res.json();
      const rows = json.providers.map(p => {
        const row = document.createElement('tr');
        const provider = document.createElement('td');
        provider.textContent = p.provider;
        const status = document.createElement('td');
        status.className = p.status;
        status.append(document.createTextNode(p.status));
        status.append(document.createElement('br'));
        const message = document.createElement('small');
        message.textContent = p.message || '';
        status.append(message);
        row.append(provider, status);
        return row;
      });
      health.replaceChildren(...rows);
    }
    document.getElementById('routeBtn').onclick = () => post('/route');
    document.getElementById('runBtn').onclick = () => post('/run');
    document.getElementById('queueBtn').onclick = () => post('/queue');
    document.getElementById('refreshHealth').onclick = loadHealth;
    loadHealth();
  </script>
</body>
</html>"#;
