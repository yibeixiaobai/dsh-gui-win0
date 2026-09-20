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
  updateCheck,
  updateInstall,
  type DesktopSettings,
  type RuntimeEvent,
  type RuntimeManifest,
  type RuntimeSnapshot,
  type SessionSummary,
  type UpdateInfo
} from "./api";

type ChatMessage = { id: string; role: "user" | "assistant"; text: string };
type ApprovalNotice = { id: string; toolName: string; sessionId: string };

function jsonText(value: unknown): string {
  if (typeof value === "string") return value;
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}

function extractEvent(value: unknown): { text: string | null; type: string | null; data: Record<string, unknown> | null } {
  if (!value || typeof value !== "object") return { text: null, type: null, data: null };
  const raw = (value as Record<string, unknown>).event;
  if (!raw || typeof raw !== "object") return { text: null, type: null, data: null };
  const event = raw as Record<string, unknown>;
  const type = typeof event.type === "string" ? event.type : null;
  const data = event.data && typeof event.data === "object" ? event.data as Record<string, unknown> : null;

  if (type === "assistant/message" && data?.message && typeof data.message === "object") {
    const message = data.message as Record<string, unknown>;
    const content = message.content;
    if (Array.isArray(content)) {
      const texts = content
        .filter(block => block && typeof block === "object" && (block as Record<string, unknown>).type === "text")
        .map(block => String((block as Record<string, unknown>).text ?? ""))
        .filter(Boolean);
      return { text: texts.length ? texts.join("") : null, type, data };
    }
  }

  return { text: null, type, data };
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
  const [approval, setApproval] = useState<ApprovalNotice | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
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
      const parsed = extractEvent(event.payload);

      if (event.kind === "jsonrpc" && parsed.text) {
        setMessages(current => [...current, { id: crypto.randomUUID(), role: "assistant", text: parsed.text! }]);
      }

      const payload = event.payload as Record<string, unknown> | null;
      const incomingSessionId = payload && typeof payload === "object" && typeof payload.sessionId === "string"
        ? payload.sessionId
        : null;

      if (parsed.type === "approval/asked" && incomingSessionId && parsed.data) {
        const id = typeof parsed.data.id === "string" ? parsed.data.id : crypto.randomUUID();
        const toolName = typeof parsed.data.toolName === "string" ? parsed.data.toolName : "unknown tool";
        setApproval({ id, toolName, sessionId: incomingSessionId });
      }

      if (parsed.type === "approval/decided" && parsed.data && typeof parsed.data.id === "string") {
        setApproval(current => current?.id === parsed.data!.id ? null : current);
      }

      if (incomingSessionId) {
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

      const serverInfo = (response as { result?: { serverInfo?: { name?: string; version?: string } } })?.result?.serverInfo;
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
    try {
      await runtimeStop();
      await sessionSetStatus(sessionId, "stopped").catch(() => undefined);
      setSnapshot(await runtimeStatus());
    } catch (cause) { setError(String(cause)); }
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
    } catch (cause) { setError(String(cause)); }
  }

  async function healthCheck() {
    try {
      const result = await runtimeHealthCheck();
      setError(result.ok ? null : result.reason);
      setSnapshot(await runtimeStatus());
    } catch (cause) { setError(String(cause)); }
  }

  async function checkUpdates() {
    setCheckingUpdate(true);
    setError(null);
    try { setUpdate(await updateCheck()); }
    catch (cause) { setError(String(cause)); }
    finally { setCheckingUpdate(false); }
  }

  async function installUpdate() {
    if (!update) return;
    setCheckingUpdate(true);
    try { await updateInstall(update); }
    catch (cause) {
      setError(String(cause));
      setCheckingUpdate(false);
    }
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
                <button className="session-delete" title="仅移除 Desktop 索引，不删除 Harness 会话数据" onClick={() => void forgetSession(session)}>×</button>
              </div>
            ))}
          </div>
          {sessions.length === 0 && <div className="muted">尚无 Desktop 索引；会话正文由 Harness 持久化。</div>}
        </section>

        <section className="side-card">
          <div className="label">Runtime</div>
          <div className="status-row">
            <span className={"status-dot " + (ready ? "online" : "")} />
            <strong>{snapshot?.status ?? "loading"}</strong>
          </div>
          <div className="muted">{manifest?.harnessVersion ?? "manifest unavailable"}</div>
          <button className="ghost-button" onClick={() => void checkUpdates()}>
            {checkingUpdate ? "检查中..." : "检查更新"}
          </button>
          {update && (
            <div className="update-card">
              <div><strong>发现 DSH Desktop {update.version}</strong></div>
              <button className="primary-button" onClick={() => void installUpdate()}>下载并安装</button>
            </div>
          )}
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

        {approval && (
          <div className="approval-banner">
            <strong>Runtime 请求审批</strong>
            <span>{approval.toolName} · request {approval.id}</span>
            <small>当前公开 SDK JSON-RPC 没有 approval/respond 方法。Desktop 不会伪造未公开协议，只展示 Harness 的 durable approval 事件。</small>
          </div>
        )}

        {error && <div className="error-banner">{error}</div>}

        <section className="chat-panel">
          <div className="chat-scroll">
            {messages.length === 0 ? (
              <div className="empty-state">
                <div className="empty-icon">◎</div>
                <h2>Desktop Host 已准备</h2>
                <p>Desktop 只维护会话索引，不复制 Harness transcript；Workspace、DSH_HOME 和临时目录彼此隔离。</p>
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
