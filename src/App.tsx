import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AssetGroup } from "./components/AssetGroup";
import { TaskQueue } from "./components/TaskQueue";
import { ExportPanel } from "./components/ExportPanel";
import { PerformanceBar } from "./components/PerformanceBar";
import { ConfirmDialog, type DialogRequest } from "./components/ConfirmDialog";
import { useWorkbench } from "./hooks/useWorkbench";
import { api, desktopAvailable } from "./lib/api";
import type { Settings, TaskView } from "./lib/contracts";

// 页面仅编排组件和用户动作，媒体计算、调度与恢复由 Rust 负责。
export default function App() {
  const workbench = useWorkbench();
  const {
    snapshot: s,
    environment,
    performance,
    error,
    busy,
    page,
    setPage,
    run,
  } = workbench;
  const [dialog, setDialog] = useState<DialogRequest | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const current = useRef(workbench);
  current.current = workbench;
  const closePending = useRef(false);
  const settings: Settings = s?.settings ?? {
    ratio: "follow-b",
    transition: "none",
  };
  const action = (callback: () => Promise<void>) => {
    void run(callback);
  };

  useEffect(() => {
    if (!desktopAvailable) return;
    const releaseClose = listen("close-requested", () => {
      if (closePending.current) return;
      const w = current.current;
      if (w.snapshot?.summary.locked) {
        closePending.current = true;
        setDialog({
          title: "停止并退出？",
          body: "当前仍有未完成任务。退出会停止正在处理的视频，并将恢复记录保存在导出目录。\n下次手动选择恢复记录即可继续；中断项从头处理，完成项保留。",
          confirm: "停止并退出",
          action: () => {
            void w.run(api.exit);
          },
        });
      } else {
        void w.run(api.exit);
      }
    });
    const releaseDrop = getCurrentWebview().onDragDropEvent((event) => {
      const data = event.payload;
      if (data.type === "leave") {
        setOver(null);
        return;
      }
      if (!("position" in data)) return;
      const target = document
        .elementFromPoint(
          data.position.x / window.devicePixelRatio,
          data.position.y / window.devicePixelRatio,
        )
        ?.closest<HTMLElement>("[data-drop-group]");
      const group = target?.dataset.dropGroup ?? null;
      if (data.type === "drop") {
        setOver(null);
        if (current.current.snapshot?.summary.locked || current.current.busy)
          return;
        if (group) {
          void current.current.run(() => api.drop(data.paths, group));
        } else {
          current.current.setError("请将视频拖入视频组 A 或视频组 B。");
        }
      } else {
        setOver(group);
      }
    });
    return () => {
      void releaseClose.then((f) => f());
      void releaseDrop.then((f) => f());
    };
  }, []);

  // 部分组合受阻时明确确认，正常批次无需额外批准。
  function start() {
    const request = crypto.randomUUID();
    if (s?.summary.blocked) {
      setDialog({
        title: "部分组合无法导出",
        body: `共 ${s.summary.total} 个组合，其中 ${s.summary.blocked} 个受阻。\n只导出 ${s.summary.runnable} 个正常组合？受阻项将保留原因和恢复记录。`,
        confirm: "导出正常组合",
        action: () => action(() => api.start(request, true)),
      });
    } else {
      action(() => api.start(request, false));
    }
  }

  // 取消表示放弃，仅清理所选未完成项的自有残片。
  function cancel(id: string | null = null) {
    setDialog({
      title: id ? "取消此项导出？" : "取消剩余导出？",
      body: "取消表示放弃，删除对应的进行中视频，取消项不再重试或恢复。\n已完成的视频保留。如希望以后继续，请暂停队列或停止并退出。",
      confirm: "确认取消",
      action: () => action(() => api.cancel(id)),
    });
  }

  // 详情属于本次界面状态，不创建诊断或日志文件。
  function details(task: TaskView) {
    setDialog({
      title: task.issue ? "任务需要处理" : "处理说明",
      body: [task.issue, ...task.warnings].filter(Boolean).join("\n\n"),
      confirm: "关闭",
    });
  }

  return (
    <div id="vs-terminal" aria-label="视频拼接工作台">
      <div className="vs-content">
        <header className="term-header">
          <div className="term-brandline">
            <h1>
              VIDEO STITCH
              <span className="term-caret" aria-hidden="true">
                _
              </span>
            </h1>
            <p>
              视频拼接工作台 <span className="term-muted">/</span>{" "}
              两组视频，自动组合。
            </p>
          </div>
          <button
            className="vs-text-button"
            disabled={!desktopAvailable || busy || s?.summary.locked}
            onClick={() => action(api.recover)}
          >
            恢复任务
          </button>
        </header>
        <main>
          <div className="term-section-label">
            <span>01 / INPUT</span>
            <span>先播放 A，再播放 B</span>
          </div>
          <section className="vs-imports" aria-label="添加视频">
            {(["a", "b"] as const).map((group) => (
              <AssetGroup
                key={group}
                group={group}
                assets={(group === "a" ? s?.assetsA : s?.assetsB) ?? []}
                locked={!desktopAvailable || !!s?.summary.locked}
                busy={busy}
                active={!!s?.summary.active}
                over={over === group}
                onAdd={() => action(() => api.selectAssets(group))}
                onRemove={(id) => action(() => api.remove(group, id))}
                onRelink={(id) => action(() => api.relink(id))}
              />
            ))}
          </section>
          <TaskQueue
            tasks={s?.tasks ?? []}
            summary={s?.summary}
            aCount={s?.assetsA.length ?? 0}
            bCount={s?.assetsB.length ?? 0}
            page={s?.page ?? page}
            busy={busy}
            onPage={setPage}
            onOpen={(id) => action(() => api.open(id))}
            onRetry={(id) => action(() => api.retry(id))}
            onDetails={details}
            onCancel={cancel}
          />
          <ExportPanel
            summary={s?.summary}
            settings={settings}
            busy={busy}
            environment={environment}
            enabled={desktopAvailable && !!s}
            onSettings={(settings) => action(() => api.configure(settings))}
            onDirectory={() => action(api.directory)}
            onStart={start}
            onPause={() => action(api.pause)}
            onResume={() => action(api.resume)}
            onRetry={() => action(api.retry)}
            onCancel={() => cancel()}
            onOpen={() => action(api.open)}
          />
          <div
            className={`vs-notice ${error ? "has-error" : ""}`}
            role="status"
            aria-live="polite"
          >
            {!desktopAvailable
              ? "当前为界面预览。请使用桌面应用添加本地视频并执行真实导出（npm run desktop）。"
              : (error ?? s?.summary.notice)}
          </div>
        </main>
      </div>
      <PerformanceBar
        value={performance}
        environment={environment}
        summary={s?.summary}
      />
      <ConfirmDialog
        request={dialog}
        onClose={() => {
          setDialog(null);
          closePending.current = false;
        }}
      />
    </div>
  );
}
