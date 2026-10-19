import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  EncoderStatus,
  Performance,
  Settings,
  Snapshot,
} from "./contracts";

export const desktopAvailable = isTauri();

// IPC 只暴露具体应用动作，前端不传 shell 参数或任意输出路径。
export const api = {
  snapshot: (page: number) => invoke<Snapshot>("get_snapshot", { page }),
  environment: () => invoke<EncoderStatus>("get_environment_status"),
  performance: () => invoke<Performance>("get_performance_snapshot"),
  selectAssets: (group: string) => invoke<void>("select_assets", { group }),
  drop: (paths: string[], group: string) =>
    invoke<void>("import_dropped", { paths, group }),
  remove: (group: string, id: string | null = null) =>
    invoke<void>("remove_assets", { group, id }),
  configure: (settings: Settings) => invoke<void>("configure", { settings }),
  directory: () => invoke<void>("select_directory"),
  start: (requestId: string, allowBlocked: boolean) =>
    invoke<void>("start_batch", { requestId, allowBlocked }),
  pause: () => invoke<void>("pause_batch"),
  resume: () => invoke<void>("resume_batch"),
  cancel: (id: string | null = null) => invoke<void>("cancel_tasks", { id }),
  retry: (id: string | null = null) => invoke<void>("retry_tasks", { id }),
  recover: () => invoke<void>("select_recovery_record"),
  relink: (id: string) => invoke<void>("relink_asset", { id }),
  open: (id: string | null = null) => invoke<void>("open_output", { id }),
  exit: () => invoke<void>("exit_app"),
};

// 原生结构化错误直接呈现为可执行提示，不建立错误历史或日志。
export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error)
    return String(error.message);
  return typeof error === "string"
    ? error
    : "操作失败，请检查文件与保存目录后重试。";
}
