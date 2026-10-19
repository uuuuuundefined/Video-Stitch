import type { Summary, TaskState, TaskView } from "../lib/contracts";
import { bytes, duration, progressBar } from "../lib/format";

const labels: Record<TaskState, string> = {
  pending: "WAIT · 待导出",
  preflight: "CHECK · 预检中",
  ready: "WAIT · 已就绪",
  running: "RUN · 处理中",
  validating: "CHECK · 检查结果",
  committing: "SAVE · 提交结果",
  succeeded: "DONE · 已完成",
  blocked: "BLOCK · 输入受阻",
  failed: "ERR · 失败",
  canceling: "STOP · 取消中",
  canceled: "STOP · 已取消",
  interrupted: "WAIT · 上次中断",
};

interface Props {
  tasks: TaskView[];
  summary?: Summary;
  aCount: number;
  bCount: number;
  page: number;
  busy: boolean;
  onPage: (page: number) => void;
  onOpen: (id: string) => void;
  onRetry: (id: string) => void;
  onDetails: (task: TaskView) => void;
  onCancel: (id: string) => void;
}

// 使用后端分页展示完整笛卡尔积；编码 100% 之后仍需经过校验提交。
export function TaskQueue({
  tasks,
  summary,
  aCount,
  bCount,
  page,
  busy,
  onPage,
  onOpen,
  onRetry,
  onDetails,
  onCancel,
}: Props) {
  const pages = Math.ceil((summary?.total ?? 0) / 80);
  const phase = summary?.active
    ? summary.paused
      ? "完成当前视频后暂停"
      : "正在导出"
    : summary?.paused
      ? "队列已暂停"
      : summary?.failed
        ? `${summary.failed} 个失败`
        : summary?.succeeded
          ? "处理结束"
          : summary?.total
            ? "准备就绪"
            : "等待添加";
  return (
    <section className="term-queue" aria-label="将要生成的视频">
      <div className="term-queue-head">
        <div>
          <div className="term-section-label">
            <span>02 / QUEUE</span>
          </div>
          <h2 className="term-queue-title">
            生成队列{" "}
            <span className="term-combination-count">
              {aCount} × {bCount} = {summary?.total ?? 0} 个视频
              {summary?.blocked ? ` · ${summary.blocked} 个受阻` : ""}
            </span>
          </h2>
        </div>
        <span className="vs-job-phase">{phase}</span>
      </div>
      {tasks.length ? (
        <div className="term-table-region">
          <table className="vs-table">
            <thead>
              <tr>
                <th scope="col" className="vs-index">
                  ID
                </th>
                <th scope="col">输出文件</th>
                <th scope="col">时长</th>
                <th scope="col">大小</th>
                <th scope="col">分辨率</th>
                <th scope="col">状态 / 进度</th>
                <th scope="col" className="vs-actions-column">
                  操作
                </th>
              </tr>
            </thead>
            <tbody>
              {tasks.map((task, index) => (
                <tr key={task.id} data-job-state={task.state}>
                  <td className="vs-index">
                    {String(page * 80 + index + 1).padStart(2, "0")}
                  </td>
                  <td>
                    <span className="vs-output-name" title={task.name}>
                      {task.name}
                    </span>
                    <span
                      className="vs-row-sub"
                      title={`${task.sourceA} ＋ ${task.sourceB}`}
                    >
                      {task.sourceA} ＋ {task.sourceB}
                    </span>
                  </td>
                  <td data-label="时长">
                    {task.result ? "" : "预计 "}
                    {duration(task.result?.seconds ?? task.seconds)}
                  </td>
                  <td data-label="大小">
                    {task.result ? bytes(task.result.bytes) : "暂无法估算"}
                  </td>
                  <td data-label="分辨率">
                    {task.result
                      ? `${task.result.width} × ${task.result.height}`
                      : task.canvas
                        ? `${task.canvas.width} × ${task.canvas.height}`
                        : "待确认"}
                  </td>
                  <td className="vs-status-column">
                    <div className="vs-row-status">
                      <span>{labels[task.state]}</span>
                    </div>
                    <div
                      className="vs-progress"
                      role="progressbar"
                      aria-label={`${task.name} 导出进度`}
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={Math.floor(task.progress)}
                    >
                      <span aria-hidden="true">
                        {progressBar(task.progress)}
                      </span>
                      <span className="term-progress-percent">
                        {task.state === "blocked"
                          ? "—"
                          : `${Math.floor(task.progress)}%`}
                      </span>
                    </div>
                  </td>
                  <td className="vs-actions-column" data-label="操作">
                    <div className="vs-row-actions">
                      {task.state === "succeeded" ? (
                        <button
                          className="vs-text-button"
                          disabled={busy}
                          onClick={() => onOpen(task.id)}
                        >
                          打开视频
                        </button>
                      ) : null}
                      {task.issue || task.warnings.length ? (
                        <button
                          className="vs-text-button"
                          onClick={() => onDetails(task)}
                        >
                          {task.issue ? "查看原因" : "处理说明"}
                        </button>
                      ) : null}
                      {["failed", "interrupted"].includes(task.state) ? (
                        <button
                          className="vs-text-button"
                          disabled={busy || summary?.active}
                          onClick={() => onRetry(task.id)}
                        >
                          重试
                        </button>
                      ) : null}
                      {summary?.recoveryPath &&
                      !["succeeded", "canceled", "canceling"].includes(
                        task.state,
                      ) ? (
                        <button
                          className="vs-text-button"
                          disabled={busy}
                          onClick={() => onCancel(task.id)}
                        >
                          取消此项
                        </button>
                      ) : null}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="vs-empty">
          <span aria-hidden="true">[ - ]</span>
          <p>等待输入：请添加视频组 A 和 B</p>
        </div>
      )}
      {pages > 1 ? (
        <nav className="vs-pagination" aria-label="任务分页">
          <button
            className="vs-text-button"
            disabled={page === 0}
            onClick={() => onPage(page - 1)}
          >
            上一页
          </button>
          <span>
            {page + 1} / {pages} · 每页 80 项
          </span>
          <button
            className="vs-text-button"
            disabled={page + 1 >= pages}
            onClick={() => onPage(page + 1)}
          >
            下一页
          </button>
        </nav>
      ) : null}
    </section>
  );
}
