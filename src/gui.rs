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
      --good: #8bd17c;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font: 14px/1.45 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    main { width: min(1380px, calc(100vw - 32px)); margin: 20px auto; }
    header { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
    h1 { margin: 0; font-size: 24px; letter-spacing: 0; }
    h2 { margin: 0 0 10px; font-size: 16px; letter-spacing: 0; }
    h3 { margin: 14px 0 8px; font-size: 13px; color: var(--muted); font-weight: 600; }
    .tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 14px; }
    .tab {
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #202632;
      color: var(--text);
      padding: 8px 11px;
      font: inherit;
      cursor: pointer;
    }
    .tab.active { border-color: var(--accent); color: var(--accent); background: #122b2a; }
    .view { display: none; }
    .view.active { display: block; }
    .grid { display: grid; grid-template-columns: minmax(0, 1fr) 380px; gap: 14px; align-items: start; }
    .two { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
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
    textarea { min-height: 190px; resize: vertical; }
    textarea.config-editor { min-height: 560px; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
    .row { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; margin: 12px 0; }
    .compact-row { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; margin: 10px 0; }
    .actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
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
    button:disabled { color: var(--muted); cursor: not-allowed; border-color: var(--line); }
    pre, .log {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      background: #0d0f13;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 12px;
      min-height: 160px;
    }
    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; border-bottom: 1px solid var(--line); padding: 8px 6px; vertical-align: top; }
    th { color: var(--muted); font-weight: 600; }
    small { color: var(--muted); }
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
    .status-pill {
      display: inline-flex;
      align-items: center;
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 3px 8px;
      color: var(--muted);
      margin: 2px 4px 2px 0;
    }
    .available, .good { color: var(--good); }
    .unavailable, .bad { color: var(--bad); }
    .disabled, .warn { color: var(--warn); }
    .path-picker-control { display: flex; gap: 8px; }
    .path-picker-control input { min-width: 0; }
    .path-picker-control button { flex: 0 0 auto; }
    .picker {
      margin: 12px 0;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #0d0f13;
      overflow: hidden;
    }
    .picker[hidden] { display: none; }
    .picker-bar {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px;
      border-bottom: 1px solid var(--line);
    }
    .picker-current {
      flex: 1;
      min-width: 0;
      color: var(--muted);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .path-entries { max-height: 260px; overflow: auto; padding: 6px; }
    .path-entry {
      display: grid;
      grid-template-columns: 72px minmax(0, 1fr);
      gap: 8px;
      align-items: center;
      width: 100%;
      text-align: left;
      background: transparent;
      border-color: transparent;
      padding: 7px 8px;
    }
    .path-entry:hover { background: #171a20; }
    .path-entry[disabled] { cursor: default; color: var(--muted); }
    .path-kind { color: var(--muted); }
    .path-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    @media (max-width: 940px) {
      main { width: min(100vw - 20px, 780px); margin-top: 12px; }
      .grid, .two, .row, .compact-row { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <main>
    <header>
      <h1>Model Router</h1>
      <div class="actions">
        <button id="refreshAll" type="button">Refresh</button>
        <a href="/health"><button type="button">Health JSON</button></a>
      </div>
    </header>
    <nav class="tabs" aria-label="Views">
      <button class="tab active" data-view="routeView" type="button">Route</button>
      <button class="tab" data-view="providersView" type="button">Providers</button>
      <button class="tab" data-view="configView" type="button">Config</button>
      <button class="tab" data-view="projectsView" type="button">Projects</button>
      <button class="tab" data-view="historyView" type="button">History</button>
      <button class="tab" data-view="daemonView" type="button">Daemon</button>
    </nav>

    <section id="routeView" class="view active">
      <div class="grid">
        <section class="panel">
          <label for="prompt">Prompt</label>
          <textarea id="prompt">Fix the failing parser tests in this repo.</textarea>
          <div class="row">
            <div>
              <label for="prefer">Provider</label>
              <select id="prefer"></select>
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
              <div class="path-picker-control">
                <input id="cwd" placeholder="/path/to/project">
                <button id="browsePath" type="button">Browse</button>
              </div>
            </div>
          </div>
          <div id="pathPicker" class="picker" hidden>
            <div class="picker-bar">
              <button id="parentPath" type="button">Up</button>
              <span id="currentPath" class="picker-current"></span>
              <button id="usePath" type="button">Use</button>
              <button id="closePath" type="button">Close</button>
            </div>
            <div id="pathEntries" class="path-entries"></div>
          </div>
          <div class="actions">
            <button class="primary" id="routeBtn" type="button">Route</button>
            <button id="runBtn" type="button">Run</button>
            <button id="queueBtn" type="button">Queue</button>
          </div>
          <h3>Output</h3>
          <pre id="output"></pre>
        </section>
        <aside class="panel">
          <h2>Decision</h2>
          <div id="decisionSummary" class="log">No decision yet.</div>
          <h2>Spend</h2>
          <div id="metrics">No metrics loaded.</div>
          <h2>Queue</h2>
          <pre id="queue">No queued jobs yet.</pre>
        </aside>
      </div>
    </section>

    <section id="providersView" class="view">
      <div class="panel">
        <h2>Providers</h2>
        <table>
          <thead><tr><th>Provider</th><th>Enabled</th><th>Model</th><th>Endpoint</th><th>Status</th><th>Actions</th></tr></thead>
          <tbody id="providers"></tbody>
        </table>
      </div>
    </section>

    <section id="configView" class="view">
      <div class="grid">
        <section class="panel">
          <h2>Config</h2>
          <textarea id="configText" class="config-editor" spellcheck="false"></textarea>
          <div class="actions">
            <button id="loadConfig" type="button">Load</button>
            <button id="validateConfig" type="button">Validate</button>
            <button class="primary" id="saveConfig" type="button">Save and reload</button>
          </div>
        </section>
        <aside class="panel">
          <h2>Status</h2>
          <pre id="configStatus">No config loaded.</pre>
        </aside>
      </div>
    </section>

    <section id="projectsView" class="view">
      <div class="two">
        <section class="panel">
          <h2>Project Profiles</h2>
          <table>
            <thead><tr><th>Name</th><th>Path match</th><th>Default</th><th>Code</th><th>Reasoning</th><th>Local</th></tr></thead>
            <tbody id="profiles"></tbody>
          </table>
        </section>
        <aside class="panel">
          <h2>Add or update profile</h2>
          <label for="profileName">Name</label>
          <input id="profileName" placeholder="miladapp">
          <label for="profilePath">Path contains</label>
          <div class="path-picker-control">
            <input id="profilePath" placeholder="MiladApp">
            <button id="browseProfilePath" type="button">Browse</button>
          </div>
          <div class="compact-row">
            <div><label for="profileDefault">Default</label><select id="profileDefault"></select></div>
            <div><label for="profileCode">Code</label><select id="profileCode"></select></div>
            <div><label for="profileReasoning">Reasoning</label><select id="profileReasoning"></select></div>
            <div><label for="profileLocal">Local</label><select id="profileLocal"></select></div>
          </div>
          <button class="primary" id="saveProfile" type="button">Save profile</button>
          <pre id="profileStatus"></pre>
        </aside>
      </div>
    </section>

    <section id="historyView" class="view">
      <div class="panel">
        <h2>History</h2>
        <table>
          <thead><tr><th>When</th><th>Operation</th><th>Provider</th><th>Model</th><th>Cost</th><th>Status</th><th>Why</th></tr></thead>
          <tbody id="history"></tbody>
        </table>
      </div>
    </section>

    <section id="daemonView" class="view">
      <div class="two">
        <section class="panel">
          <h2>Health</h2>
          <table>
            <thead><tr><th>Provider</th><th>Status</th><th>Message</th></tr></thead>
            <tbody id="health"></tbody>
          </table>
        </section>
        <aside class="panel">
          <h2>Setup</h2>
          <code>modelrouter init</code>
          <code>modelrouter doctor</code>
          <code>modelrouter daemon start --open</code>
          <code>modelrouter tui</code>
          <h2>MCP</h2>
          <code>modelrouter mcp install-config --config modelrouter.toml</code>
        </aside>
      </div>
    </section>
  </main>

  <script>
    const state = { config: null, health: [], metrics: null, selectedPath: '' };
    const providerIds = ['local', 'claude', 'codex', 'gemini', 'lmstudio', 'llamacpp', 'openai_compatible', 'aider'];
    const output = document.getElementById('output');
    const cwdInput = document.getElementById('cwd');
    const pathPicker = document.getElementById('pathPicker');
    const currentPath = document.getElementById('currentPath');
    const pathEntries = document.getElementById('pathEntries');

    function option(value, label, selected) {
      const item = document.createElement('option');
      item.value = value;
      item.textContent = label;
      if (selected) item.selected = true;
      return item;
    }

    function fillProviderSelect(select, includeAuto) {
      select.replaceChildren();
      if (includeAuto) select.append(option('', 'auto', true));
      for (const id of providerIds) select.append(option(id, id, false));
    }

    function setJson(target, value) {
      target.textContent = JSON.stringify(value, null, 2);
    }

    async function api(path, options = {}) {
      const res = await fetch(path, options);
      const text = await res.text();
      const json = text ? JSON.parse(text) : {};
      if (!res.ok) throw json;
      return json;
    }

    function payload() {
      const body = { prompt: document.getElementById('prompt').value };
      for (const id of ['prefer', 'hint', 'cwd']) {
        const value = document.getElementById(id).value.trim();
        if (value) body[id] = value;
      }
      return body;
    }

    function renderDecision(decision) {
      const target = document.getElementById('decisionSummary');
      if (!decision || !decision.provider) {
        target.textContent = 'No decision yet.';
        return;
      }
      const reasons = (decision.reasons || []).map(reason => `- ${reason}`).join('\n');
      target.textContent = [
        `provider: ${decision.provider}`,
        `model: ${decision.model}`,
        `billing: ${decision.billing}`,
        `estimated cost: ${Number(decision.estimated_cost_cents || 0).toFixed(6)} cents`,
        `confidence: ${Number(decision.confidence || 0).toFixed(2)}`,
        '',
        reasons
      ].join('\n');
    }

    async function post(path) {
      output.textContent = 'Working...';
      try {
        const json = await api(path, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload())
        });
        setJson(output, json);
        renderDecision(json.decision || json);
        await loadHistory();
        await loadMetrics();
      } catch (error) {
        setJson(output, error);
      }
    }

    async function loadPath(path, onUse) {
      pathPicker.hidden = false;
      pathEntries.replaceChildren(document.createTextNode('Loading...'));
      try {
        const url = path ? `/fs?path=${encodeURIComponent(path)}` : '/fs';
        const json = await api(url);
        state.selectedPath = json.path;
        currentPath.textContent = json.path;
        const rows = json.entries.map(entry => {
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'path-entry';
          if (entry.kind !== 'directory') button.disabled = true;
          const kind = document.createElement('span');
          kind.className = 'path-kind';
          kind.textContent = entry.kind === 'directory' ? 'folder' : 'file';
          const name = document.createElement('span');
          name.className = 'path-name';
          name.textContent = entry.name;
          button.append(kind, name);
          if (entry.kind === 'directory') button.onclick = () => loadPath(entry.path, onUse);
          return button;
        });
        pathEntries.replaceChildren(...(rows.length ? rows : [document.createTextNode('No entries.')]));
        document.getElementById('parentPath').disabled = !json.parent;
        document.getElementById('parentPath').onclick = () => { if (json.parent) loadPath(json.parent, onUse); };
        document.getElementById('usePath').onclick = () => {
          onUse(state.selectedPath);
          pathPicker.hidden = true;
        };
      } catch (error) {
        pathEntries.replaceChildren(document.createTextNode(error.message || error.error || 'Unable to read path.'));
      }
    }

    async function loadConfig() {
      const json = await api('/config');
      state.config = json.config;
      document.getElementById('configText').value = json.toml || '';
      renderProviders();
      renderProfiles();
      document.getElementById('configStatus').textContent = 'Loaded current daemon config.';
    }

    async function validateConfig() {
      try {
        const json = await api('/config/validate', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ toml: document.getElementById('configText').value })
        });
        setJson(document.getElementById('configStatus'), json);
      } catch (error) {
        setJson(document.getElementById('configStatus'), error);
      }
    }

    async function saveConfig() {
      try {
        const json = await api('/config', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ toml: document.getElementById('configText').value })
        });
        state.config = json.config;
        document.getElementById('configText').value = json.toml || document.getElementById('configText').value;
        setJson(document.getElementById('configStatus'), json);
        await refreshAll();
      } catch (error) {
        setJson(document.getElementById('configStatus'), error);
      }
    }

    async function loadHealth() {
      const json = await api('/health');
      state.health = json.providers || [];
      const rows = state.health.map(provider => {
        const row = document.createElement('tr');
        for (const value of [provider.provider, provider.status, provider.message || '']) {
          const cell = document.createElement('td');
          cell.textContent = value;
          if (value === provider.status) cell.className = provider.status;
          row.append(cell);
        }
        return row;
      });
      document.getElementById('health').replaceChildren(...rows);
      renderProviders();
    }

    async function loadMetrics() {
      const json = await api('/metrics');
      state.metrics = json;
      document.getElementById('metrics').textContent = `${json.total_requests} requests\n${Number(json.estimated_cost_cents || 0).toFixed(6)} estimated cents`;
    }

    async function loadHistory() {
      const json = await api('/history');
      const rows = (json.entries || []).map(entry => {
        const row = document.createElement('tr');
        const cells = [
          new Date(Number(entry.timestamp_unix_ms)).toLocaleString(),
          entry.operation,
          entry.provider,
          entry.model,
          Number(entry.actual_cost_cents ?? entry.estimated_cost_cents ?? 0).toFixed(6),
          entry.success ? 'ok' : 'failed',
          (entry.reasons || []).join(' ')
        ];
        for (const value of cells) {
          const cell = document.createElement('td');
          cell.textContent = value;
          row.append(cell);
        }
        return row;
      });
      document.getElementById('history').replaceChildren(...rows);
    }

    function renderProviders() {
      if (!state.config) return;
      const healthByProvider = new Map(state.health.map(item => [item.provider, item]));
      const rows = state.config.providers.map(provider => {
        const health = healthByProvider.get(provider.id);
        const row = document.createElement('tr');
        const name = document.createElement('td');
        name.textContent = provider.id;
        const enabled = document.createElement('td');
        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = provider.enabled;
        enabled.append(checkbox);
        const model = document.createElement('td');
        const modelInput = document.createElement('input');
        modelInput.value = provider.model;
        model.append(modelInput);
        const endpoint = document.createElement('td');
        const endpointInput = document.createElement('input');
        endpointInput.value = provider.endpoint_url || '';
        endpoint.append(endpointInput);
        const status = document.createElement('td');
        status.textContent = health ? `${health.status}: ${health.message}` : 'not checked';
        if (health) status.className = health.status;
        const actions = document.createElement('td');
        const save = document.createElement('button');
        save.type = 'button';
        save.textContent = 'Save';
        save.onclick = () => saveProvider(provider.id, checkbox.checked, modelInput.value, endpointInput.value);
        const test = document.createElement('button');
        test.type = 'button';
        test.textContent = 'Test';
        test.onclick = () => testProvider(provider.id, status);
        actions.append(save, test);
        row.append(name, enabled, model, endpoint, status, actions);
        return row;
      });
      document.getElementById('providers').replaceChildren(...rows);
    }

    async function saveProvider(provider, enabled, model, endpointUrl) {
      try {
        const json = await api('/config/provider', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ provider, enabled, model, endpoint_url: endpointUrl })
        });
        state.config = json.config;
        document.getElementById('configText').value = json.toml || document.getElementById('configText').value;
        await refreshAll();
      } catch (error) {
        alert(error.message || error.error || 'Provider save failed.');
      }
    }

    async function testProvider(provider, target) {
      target.textContent = 'checking...';
      try {
        const json = await api('/provider-test', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ provider })
        });
        target.textContent = `${json.status}: ${json.message}`;
        target.className = json.status;
      } catch (error) {
        target.textContent = error.message || error.error || 'test failed';
        target.className = 'bad';
      }
    }

    function renderProfiles() {
      if (!state.config) return;
      const rows = (state.config.profiles || []).map(profile => {
        const row = document.createElement('tr');
        for (const value of [
          profile.name,
          profile.path_contains,
          profile.default_provider || '',
          profile.code_provider || '',
          profile.reasoning_provider || '',
          profile.local_provider || ''
        ]) {
          const cell = document.createElement('td');
          cell.textContent = value;
          row.append(cell);
        }
        return row;
      });
      document.getElementById('profiles').replaceChildren(...rows);
    }

    async function saveProfile() {
      const payload = {
        name: document.getElementById('profileName').value.trim(),
        path_contains: document.getElementById('profilePath').value.trim(),
        default_provider: document.getElementById('profileDefault').value || null,
        code_provider: document.getElementById('profileCode').value || null,
        reasoning_provider: document.getElementById('profileReasoning').value || null,
        local_provider: document.getElementById('profileLocal').value || null
      };
      try {
        const json = await api('/config/profile', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload)
        });
        state.config = json.config;
        document.getElementById('configText').value = json.toml || document.getElementById('configText').value;
        setJson(document.getElementById('profileStatus'), json);
        await refreshAll();
      } catch (error) {
        setJson(document.getElementById('profileStatus'), error);
      }
    }

    async function refreshAll() {
      await loadHealth();
      await loadConfig();
      await loadMetrics();
      await loadHistory();
    }

    for (const select of [
      document.getElementById('prefer'),
      document.getElementById('profileDefault'),
      document.getElementById('profileCode'),
      document.getElementById('profileReasoning'),
      document.getElementById('profileLocal')
    ]) fillProviderSelect(select, select.id === 'prefer');

    document.querySelectorAll('.tab').forEach(tab => {
      tab.onclick = () => {
        document.querySelectorAll('.tab').forEach(item => item.classList.toggle('active', item === tab));
        document.querySelectorAll('.view').forEach(view => view.classList.toggle('active', view.id === tab.dataset.view));
      };
    });
    document.getElementById('routeBtn').onclick = () => post('/route');
    document.getElementById('runBtn').onclick = () => post('/run');
    document.getElementById('queueBtn').onclick = () => post('/queue');
    document.getElementById('browsePath').onclick = () => loadPath(cwdInput.value.trim(), value => { cwdInput.value = value; });
    document.getElementById('browseProfilePath').onclick = () => loadPath('', value => { document.getElementById('profilePath').value = value; });
    document.getElementById('closePath').onclick = () => { pathPicker.hidden = true; };
    document.getElementById('refreshAll').onclick = refreshAll;
    document.getElementById('loadConfig').onclick = loadConfig;
    document.getElementById('validateConfig').onclick = validateConfig;
    document.getElementById('saveConfig').onclick = saveConfig;
    document.getElementById('saveProfile').onclick = saveProfile;
    refreshAll().catch(error => { output.textContent = JSON.stringify(error, null, 2); });
  </script>
</body>
</html>"#;
