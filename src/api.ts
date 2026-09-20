import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type RuntimeStatus = "stopped" | "starting" | "running" | "stopping" | "crashed";

export interface RuntimeSnapshot {
  status: RuntimeStatus;
  pid: number | null;
  runtimeVersion: string;
  harnessHome: string;
  workspace: string | null;
  executable: string | null;
  lastError: string | null;
}

export interface DesktopSettings {
  provider: string;
  model: string;
  reasoningEffort?: string;
  workspace: string | null;
}

export interface RuntimeManifest {
  desktopRuntimeApi: number;
  harnessVersion: string;
  runtimeArtifact: string;
  platform: string;
  architecture: string;
  protocol: {
    name: "deepseek-harness-sdk-runtime";
    serverVersion?: string;
    methods: string[];
    notifications: string[];
  };
  sha256?: string | null;
  releaseChannel: "stable" | "preview";
}

export interface RuntimeEvent {
  kind: "jsonrpc" | "stderr" | "terminated" | "error";
  payload: unknown;
}

export interface SessionSummary {
  id: string;
  title: string;
  workspace: string;
  createdAt: number;
  lastUsedAt: number;
  status: string;
}

export interface UpdateInfo {
  version: string;
  tag: string;
  releaseUrl: string;
  installerUrl: string;
  checksumUrl?: string | null;
  prerelease: boolean;
}

export const runtimeStatus = () => invoke<RuntimeSnapshot>("runtime_status");
export const runtimeStart = (workspace: string, settings: DesktopSettings) => invoke<RuntimeSnapshot>("runtime_start", { workspace, settings });
export const runtimeStop = () => invoke<RuntimeSnapshot>("runtime_stop");
export const runtimeRequest = (method: string, params?: Record<string, unknown>) => invoke<unknown>("runtime_request", { method, params });
export const runtimeHealthCheck = () => invoke<{ ok: boolean; reason: string }>("runtime_health_check");
export const getSettings = () => invoke<DesktopSettings>("settings_get");
export const saveSettings = (settings: DesktopSettings) => invoke<void>("settings_save", { settings });
export const runtimeManifest = () => invoke<RuntimeManifest>("runtime_manifest");

export const sessionsList = () => invoke<SessionSummary[]>("sessions_list");
export const sessionRegister = (id: string, workspace: string, title?: string) => invoke<SessionSummary>("session_register", { id, workspace, title });
export const sessionSetStatus = (id: string, status: string) => invoke<void>("session_set_status", { id, status });
export const sessionForget = (id: string) => invoke<void>("session_forget", { id });

export const updateCheck = () => invoke<UpdateInfo | null>("update_check");
export const updateInstall = (info: UpdateInfo) => invoke<void>("update_install", { info });

export function listenRuntimeOutput(handler: (payload: RuntimeEvent) => void): Promise<UnlistenFn> {
  return listen<RuntimeEvent>("runtime:event", event => handler(event.payload));
}
