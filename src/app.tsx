import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  getSettings,
  listenRuntimeOutput,
  runtimeHealthCheck,
  runtimeManifest,
  runtimeRequest,
  runtimeStart,
  runtimeStatus,
  runtimeStop,
  saveSettings,
  sessionsList,
  sessionRegister,
  sessionForget,
  sessionSetStatus,
  type DesktopSettings,
  type RuntimeEvent,
  type RuntimeManifest,
  type RuntimeSnapshot,
  type SessionSummary
} from "./api";

type ChatMessage = { id: string; role: "user" | "assistant"; text: string };

function jsonText(value: unknown): string {
  if (typeof value === "string") return value;
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}

function extractEventText(value: unknown): { text: string | null; type: string | null } {
  if (!value || typeof value !== "object") return { text: null, type: null };
  const event = (value as Record<string, unknown>).event;
  if (!event || typeof event !== "object") return { text: null, type: null };
  const record = event as Record<string, unknown>;
  const type = typeof record.type === "string" ? record.type : null;
  if (typeof record.text === "string") return { text: record.text, type };
  if (Array.isArray(record.content)) {
    const texts = record.content
      .filter(block => block && typeof block === "object" && (block as Record<string, unknown>).type === "text")
      .map(block => String((block as Record<string, unknown>).text ?? ""))
      .filter(Boolean);
    return { text: texts.length ? texts.join("") : null, type };
  }
  return { text: null, type };
}

export default function App() {
  const [snapshot, setSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [manifest, setManifest] = useState<RuntimeManifest | null>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [settings, setSettings] = useState<DesktopSettings>({
    provider: "deepseek-official",
    model: "",
    reasoningEffort: "",
    workspace: null
  });
  const [sessionId, setSessionId] = useState(() => crypto.randomUUID());
  const [prompt, setPrompt] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ready = snapshot?.status === "running";
  const workspaceName = useMemo(() => {
    if (!settings.workspace) return "未选择工作区";
    const parts = settings.workspace.replaceAll("\\", "/").split("/");
    return parts.at(-1) || settings.workspace;
  }, [settings.workspace]);

  useEffect(() => {
    void Promise.all([runtimeStatus(), runtimeManifest(), getSettings(), sessionsList()])
      .then(([status, nextManifest, nextSettings, nextSessions]) => {
        setSnapshot(status);
        setManifest(nextManifest);
        setSettings(nextSettings);
        setSessions(nextSessions);
        const matching = nextSessions.find(item => item.workspace === nextSettings.workspace);
        if (matching) setSessionId(matching.id);
      })
      .catch(cause => setError(String(cause)));
  }, []);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenRuntimeOutput((event: RuntimeEvent) => {
      const parsed = extractEventText(event.payload);
      if (event.kind === "jsonrpc" && parsed.text) {
        setMessages(current => [...current, { id: crypto.randomUUID(), role: "assistant", text: parsed.text! }]);
      }

      const payload = event.payload as Record<string, unknown> | null;
      const incomingSessionId = payload && typeof payload === "object" && typeof payload.sessionId === "string"
        ? payload.sessionId
        : null;

      if (incomingSessionId && parsed.type === "session/title") {
        void sessionsList().then(setSessions).catch(() => undefined);
      }

      setLogs(current => [jsonText(event.payload), ...current].slice(0, 120));
      void runtimeStatus().then(setSnapshot).catch(() => undefined);
    }).then(unlisten => { cleanup = unlisten; });

    return () => cleanup?.();
  }, []);

  async function chooseWorkspace() {
    const result = await open({ directory: true, multiple: false, title: "选择 Harness Workspace" });
    if (typeof result !== "string") return;
    const next = { ...settings, workspace: result };
    setSettings(next);
    await saveSettings(next);
    setError(null);
  }

  async function connect() {
    if (!settings.workspace) return setError("请先选择 Workspace。");
    if (!settings.provider || !settings.model) return setError("请填写 Provider 和 Model。");

    setBusy(true);
    setError(null);
    let started = false;
    try {
      await saveSettings(settings);
      await runtimeStart(settings.workspace, settings);
      started = true;

      const response = await runtimeRequest("initialize", {
        cwd: settings.workspace,
        provider: settings.provider,
        model: settings.model,
        ...(settings.reasoningEffort ? { reasoningEffort: settings.reasoningEffort } : {})
      });

      const serverInfo = (response as { result?: { serverInfo?: { name?: string } } })?.result?.serverInfo;
      if (serverInfo?.name !== "deepseek-harness-sdk-runtime") {
        throw new Error("Runtime handshake returned an unexpected server identity.");
      }

      const session = await sessionRegister(sessionId, settings.workspace, "New session");
      setSessions(current => [session, ...current.filter(item => item.id !== session.id)]);
      setSnapshot(await runtimeStatus());
    } catch (cause) {
      if (started) await runtimeStop().catch(() => undefined);
      setError(String(cause));
      setSnapshot(await runtimeStatus().catch(() => null));
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    setBusy(true);
    setError(null);
    try { setSnapshot(await runtimeStop()); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }

  async function createSession() {
    const id = crypto.randomUUID();
    setSessionId(id);
    setMessages([]);
    setPrompt("");
    if (settings.workspace) {
      try {
        const session = await sessionRegister(id, settings.workspace, "New session");
        setSessions(current => [session, ...current]);
      } catch (cause) { setError(String(cause)); }
    }
  }

  async function selectSession(session: SessionSummary) {
    if (session.workspace !== settings.workspace) {
      const next = { ...settings, workspace: session.workspace };
      setSettings(next);
      await saveSettings(next);
    }
    setSessionId(session.id);
    setMessages([]);
    await sessionSetStatus(session.id, "active").catch(() => undefined);
  }

  async function forgetSession(session: SessionSummary) {
    await sessionForget(session.id);
    setSessions(current => current.filter(item => item.id !== session.id));
    if (session.id === sessionId) await createSession();
  }

  async function sendPrompt() {
    const text = prompt.trim();
    if (!ready || !text) return;

    setPrompt("");
    setMessages(current => [...current, { id: crypto.randomUUID(), role: "user", text }]);

    try {
      await sessionRegister(sessionId, settings.workspace!, text.slice(0, 80));
      await runtimeRequest("session/prompt", {
        sessionId,
        contentBlocks: [{ type: "text", text }]
      });
      setSessions(await sessionsList());
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function healthCheck() {
    try {
      const result = await runtimeHealthCheck();
      setError(result.ok ? null : result.reason);
      setSnapshot(await runtimeStatus());
    } catch (cause) { setError(String(cause)); }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">D</div>
          <div><strong>DSH Desktop</strong><span>Native Runtime Host</span></div>
        </div>

        <section className="side-card">
          <div className="section-heading">
            <div className="label">Sessions</div>
            <button className="small-button" onClick={createSession}>＋</button>
          </div>
          <div className="session-list">
            {sessions.slice(0, 12).map(session => (
              <div className={"session-item " + (session.id === sessionId ? "selected" : "")} key={session.id}>
                <button className="session-main" onClick={() => void selectSession(session)}>
                  <strong>{session.title}</strong>
                  <span>{session.status} · {new Date(session.lastUsedAt * 1000).toLocaleString()}</span>
                </button>
                <button className="session-delete" title="移除本地索引" onClick={() => void forgetSession(session)}>×</button>
              </div>
            ))}
          </div>
          {sessions.length === 0 && <div className="muted">尚无 Desktop 索引，会话正文仍由 Harness 持久化。</div>}
        </section>

        <section className="side-card">
          <div className="label">Runtime</div>
          <div className="status-row">
            <span className={"status-dot " + (ready ? "online" : "")} />
            <strong>{snapshot?.status ?? "loading"}</strong>
          </div>
          <div className="muted">{manifest?.harnessVersion ?? "manifest unavailable"}</div>
        </section>

        <section className="side-card">
          <div className="label">Workspace</div>
          <strong>{workspaceName}</strong>
          <button className="ghost-button" onClick={chooseWorkspace}>选择 Workspace</button>
        </section>

        <section className="side-card">
          <div className="label">Route</div>
          <input value={settings.provider} onChange={event => setSettings({ ...settings, provider: event.target.value })} placeholder="Provider" />
          <input value={settings.model} onChange={event => setSettings({ ...settings, model: event.target.value })} placeholder="Model" />
          <input value={settings.reasoningEffort ?? ""} onChange={event => setSettings({ ...settings, reasoningEffort: event.target.value })} placeholder="Reasoning effort（可选）" />
        </section>

        <div className="sidebar-spacer" />
        <div className="button-row">
          <button className="primary-button" disabled={busy || ready} onClick={connect}>{busy ? "处理中..." : "启动 Runtime"}</button>
          <button className="danger-button" disabled={busy || !ready} onClick={disconnect}>停止</button>
        </div>
        <button className="ghost-button" onClick={healthCheck}>检查运行时</button>
      </aside>

      <main className="main-panel">
        <header className="topbar">
          <div>
            <div className="eyebrow">SESSION / {sessionId.slice(0, 8)}</div>
            <h1>Harness Workspace</h1>
          </div>
          <div className="topbar-meta">
            <span>DSH_HOME</span><code>{snapshot?.harnessHome ?? "—"}</code>
          </div>
        </header>

        {error && <div className="error-banner">{error}</div>}

        <section className="chat-panel">
          <div className="chat-scroll">
            {messages.length === 0 ? (
              <div className="empty-state">
                <div className="empty-icon">◎</div>
                <h2>Desktop Host 已准备</h2>
                <p>Desktop 只维护会话索引，不复制 Harness transcript；Workspace 与 DSH_HOME 也保持分离。</p>
              </div>
            ) : messages.map(message => (
              <article className={"message " + message.role} key={message.id}>
                <div className="message-role">{message.role}</div>
                <pre>{message.text}</pre>
              </article>
            ))}
          </div>

          <div className="composer">
            <textarea
              value={prompt}
              disabled={!ready}
              onChange={event => setPrompt(event.target.value)}
              onKeyDown={event => {
                if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                  event.preventDefault();
                  void sendPrompt();
                }
              }}
              placeholder={ready ? "输入消息，Ctrl/Cmd + Enter 发送" : "启动 Runtime 后开始会话"}
            />
            <button className="send-button" disabled={!ready || !prompt.trim()} onClick={sendPrompt}>发送</button>
          </div>
        </section>

        <section className="diagnostics">
          <div className="diagnostics-title">Runtime Events</div>
          <div className="log-list">
            {logs.slice(0, 12).map((line, index) => <pre key={index}>{line}</pre>)}
          </div>
        </section>
      </main>
    </div>
  );
}
