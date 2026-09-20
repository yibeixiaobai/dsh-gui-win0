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
  type DesktopSettings,
  type RuntimeEvent,
  type RuntimeManifest,
  type RuntimeSnapshot
} from "./api";

type ChatMessage = { id: string; role: "user" | "assistant"; text: string };

function jsonText(value: unknown): string {
  if (typeof value === "string") return value;
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}

function extractText(value: unknown): string | null {
  if (!value || typeof value !== "object") return null;
  const event = (value as Record<string, unknown>).event;
  if (!event || typeof event !== "object") return null;
  const record = event as Record<string, unknown>;
  if (typeof record.text === "string") return record.text;
  const content = record.content;
  if (!Array.isArray(content)) return null;
  const texts = content
    .filter(block => block && typeof block === "object" && (block as Record<string, unknown>).type === "text")
    .map(block => String((block as Record<string, unknown>).text ?? ""))
    .filter(Boolean);
  return texts.length ? texts.join("") : null;
}

export default function App() {
  const [snapshot, setSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [manifest, setManifest] = useState<RuntimeManifest | null>(null);
  const [settings, setSettings] = useState<DesktopSettings>({
    provider: "deepseek-official",
    model: "",
    reasoningEffort: "",
    workspace: null
  });
  const [sessionId] = useState(() => crypto.randomUUID());
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
    void Promise.all([runtimeStatus(), runtimeManifest(), getSettings()])
      .then(([status, nextManifest, nextSettings]) => {
        setSnapshot(status);
        setManifest(nextManifest);
        setSettings(nextSettings);
      })
      .catch(cause => setError(String(cause)));
  }, []);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenRuntimeOutput((event: RuntimeEvent) => {
      const text = extractText(event.payload);
      if (event.kind === "jsonrpc" && text) {
        setMessages(current => [...current, { id: crypto.randomUUID(), role: "assistant", text }]);
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

  async function sendPrompt() {
    const text = prompt.trim();
    if (!ready || !text) return;
    setPrompt("");
    setMessages(current => [...current, { id: crypto.randomUUID(), role: "user", text }]);
    try {
      await runtimeRequest("session/prompt", { sessionId, contentBlocks: [{ type: "text", text }] });
    } catch (cause) { setError(String(cause)); }
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
                <p>Workspace、DSH_HOME 与 Runtime 生命周期彼此隔离。Desktop 不会把自己的状态写进项目目录。</p>
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
