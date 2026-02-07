const FLUFFY_TOKEN = "fluffy_dev_token";
const expanded = new Set();

/* =========================
   UTILITIES
========================= */
async function apiRequest(url, options = {}) {
  const headers = {
    "Content-Type": "application/json",
    "X-Fluffy-Token": FLUFFY_TOKEN,
    ...(options.headers || {})
  };
  try {
    const res = await fetch(url, { ...options, headers });
    if (!res.ok) {
      const err = await res.json();
      throw new Error(err.error || "Request failed");
    }
    return await res.json();
  } catch (err) {
    console.error(`API Error (${url}):`, err);
    addLog(err.message, "error");
    return null;
  }
}

function addLog(message, level = "info") {
  const ul = document.getElementById("logs");
  if (!ul) return;
  const li = document.createElement("li");
  li.textContent = `[${new Date().toLocaleTimeString()}] ${message}`;
  li.className = `log-${level}`;
  ul.prepend(li); // Show newest at top
  // Limit log size
  while (ul.children.length > 50) ul.removeChild(ul.lastChild);
}

/* =========================
   COMMANDS
========================= */
async function confirmCommand(commandId, approve = true) {
  const data = await apiRequest("/command", {
    method: "POST",
    body: JSON.stringify(approve
      ? { Confirm: { command_id: commandId } }
      : { Cancel: { command_id: commandId } }
    )
  });

  if (data && data.ok) {
    addLog(approve ? "Command confirmed" : "Command cancelled", "action");
  }
  await fetchData(); // Refresh UI immediately
}

async function killProcess(pid, mode = "tree") {
  const msg = mode === "tree"
    ? "Terminate this process and ALL its children?"
    : "Terminate ONLY this process?";

  if (!confirm(`⚠ ${msg}\nUnsaved work may be lost.`)) return;

  addLog(`Requesting kill (${mode}) for PID ${pid}`, "action");
  const data = await apiRequest("/command", {
    method: "POST",
    body: JSON.stringify({ KillProcess: { pid } })
  });
}

async function securityAction(pid, action) {
  addLog(`Security action (${action}) for PID ${pid}`, "action");
  await apiRequest("/security_action", {
    method: "POST",
    body: JSON.stringify({ pid, action })
  });
  await fetchData();
}

/* =========================
   RENDERING
========================= */
function buildTree(processes) {
  const map = {};
  const roots = [];
  processes.forEach(p => map[p.pid] = { ...p, children: [] });
  processes.forEach(p => {
    if (p.parent_pid && map[p.parent_pid]) {
      map[p.parent_pid].children.push(map[p.pid]);
    } else {
      roots.push(map[p.pid]);
    }
  });
  return roots;
}

function sortTree(nodes) {
  nodes.sort((a, b) => b.ram_mb - a.ram_mb);
  nodes.forEach(n => sortTree(n.children));
}
function renderNode(node, container, depth = 0, suspiciousPids = new Set()) {
  const row = document.createElement("div");
  row.className = `process-row depth-${Math.min(depth, 4)}`;
  if (suspiciousPids.has(node.pid)) {
    row.style.borderLeft = "3px solid #ff4d4d";
    row.style.background = "#2a1a1a";
  }

  const left = document.createElement("div");
  if (node.children.length > 0) {
    const toggle = document.createElement("button");
    toggle.className = "toggle-btn";
    toggle.textContent = expanded.has(node.pid) ? "▼" : "▶";
    toggle.onclick = (e) => {
      e.stopPropagation();
      expanded.has(node.pid) ? expanded.delete(node.pid) : expanded.add(node.pid);
      renderTree(lastData); // Re-render tree without full fetch
    };
    left.appendChild(toggle);
  }

  const label = document.createElement("span");
  label.textContent = `${node.name} (PID ${node.pid}) — ${node.ram_mb} MB`;
  left.appendChild(label);

  const right = document.createElement("div");
  const killBtn = document.createElement("button");
  const isTree = node.children.length > 0;
  killBtn.textContent = isTree ? "Kill tree" : "Kill";
  killBtn.onclick = () => killProcess(node.pid, isTree ? "tree" : "single");
  right.appendChild(killBtn);

  row.appendChild(left);
  row.appendChild(right);
  container.appendChild(row);

  if (expanded.has(node.pid)) {
    node.children.forEach(child => renderNode(child, container, depth + 1, suspiciousPids));
  }
}

let lastData = null;
function renderTree(data) {
  if (!data || !data.system) return;
  lastData = data;
  const container = document.getElementById("processes");
  container.innerHTML = "";

  const tree = buildTree(data.system.processes.top_ram);
  sortTree(tree);

  // Add highlight for suspicious processes
  const suspiciousPids = new Set((data.security_alerts || []).map(a => a.pid));

  tree.forEach(node => renderNode(node, container, 0, suspiciousPids));
}

function renderSecurityAlerts(data) {
  const container = document.getElementById("security-alerts");
  const alerts = data.security_alerts || [];

  if (alerts.length === 0) {
    container.innerHTML = "";
    return;
  }

  container.innerHTML = "<h3>⚠ Behavioral Security Alerts</h3>";
  alerts.forEach(alert => {
    const div = document.createElement("div");
    div.className = "security-alert-item";

    div.innerHTML = `
      <h4>Suspicious Activity: ${alert.name} (PID ${alert.pid}) <span class="badge">Score: ${alert.score}</span></h4>
      <p>Reasons: ${alert.reasons.join(", ")}</p>
    `;

    const actions = document.createElement("div");
    actions.style.background = "transparent";
    actions.style.padding = "0";

    const killBtn = document.createElement("button");
    killBtn.textContent = "Kill Process";
    killBtn.onclick = () => killProcess(alert.pid);
    actions.appendChild(killBtn);

    const trustBtn = document.createElement("button");
    trustBtn.textContent = "Trust";
    trustBtn.className = "action-btn trust-btn";
    trustBtn.onclick = () => securityAction(alert.pid, "trust");
    actions.appendChild(trustBtn);

    const ignoreBtn = document.createElement("button");
    ignoreBtn.textContent = "Ignore";
    ignoreBtn.className = "action-btn";
    ignoreBtn.onclick = () => securityAction(alert.pid, "ignore");
    actions.appendChild(ignoreBtn);

    div.appendChild(actions);
    container.appendChild(div);
  });
}

function renderUI(data) {
  if (!data) return;

  // Stats
  if (data.system) {
    const sys = data.system;
    document.getElementById("system").innerText =
      `RAM: ${sys.ram.used_mb} / ${sys.ram.total_mb} MB | CPU: ${sys.cpu.usage_percent.toFixed(1)}% | Net: ↓${sys.network.received_kb} KB ↑${sys.network.transmitted_kb} KB`;

    renderSecurityAlerts(data);
    renderTree(data);
  }

  // Insights
  const insights = document.getElementById("insights");
  insights.innerHTML = "";
  (data._insights || []).forEach(text => {
    const li = document.createElement("li");
    li.textContent = text;
    insights.appendChild(li);
  });

  // Confirmations
  const warn = document.getElementById("admin-warning");
  const confs = data.pending_confirmations || [];
  if (confs.length > 0) {
    warn.style.display = "block";
    warn.innerHTML = "<h4>⚠ Action Required</h4>";
    confs.forEach(c => {
      const div = document.createElement("div");
      div.className = "confirm-item";

      const p = document.createElement("p");
      const strong = document.createElement("strong");
      strong.textContent = c.command_name;
      p.appendChild(strong);
      p.appendChild(document.createTextNode(`: ${c.details}`));
      div.appendChild(p);

      const confirmBtn = document.createElement("button");
      confirmBtn.textContent = "Confirm";
      confirmBtn.onclick = () => confirmCommand(c.command_id, true);
      div.appendChild(confirmBtn);

      const cancelBtn = document.createElement("button");
      cancelBtn.textContent = "Cancel";
      cancelBtn.className = "cancel-btn";
      cancelBtn.onclick = () => confirmCommand(c.command_id, false);
      div.appendChild(cancelBtn);

      warn.appendChild(div);
    });
  } else {
    warn.style.display = "none";
  }
}

/* =========================
   POLLING & INIT
========================= */
async function fetchData() {
  const data = await apiRequest("/status");
  if (data) renderUI(data);
}

async function fetchLogs() {
  const logs = await apiRequest("/logs");
  if (logs) {
    const ul = document.getElementById("logs");
    ul.innerHTML = "";
    logs.slice().reverse().forEach(log => {
      const li = document.createElement("li");
      li.textContent = `[${log.level.toUpperCase()}] ${log.message}`;
      li.className = `log-${log.level}`;
      ul.appendChild(li);
    });
  }
}

setInterval(fetchData, 2000);
setInterval(fetchLogs, 5000);
fetchData();
fetchLogs();
