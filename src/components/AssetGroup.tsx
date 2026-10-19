import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Asset } from "../lib/contracts";
import { bytes, duration } from "../lib/format";

interface Props {
  group: "a" | "b";
  assets: Asset[];
  locked: boolean;
  busy: boolean;
  active: boolean;
  over: boolean;
  onAdd: () => void;
  onRemove: (id?: string) => void;
  onRelink: (id: string) => void;
}

// 大输入组仅渲染可见文件；移除操作只删除引用。
export function AssetGroup({
  group,
  assets,
  locked,
  busy,
  active,
  over,
  onAdd,
  onRemove,
  onRelink,
}: Props) {
  const parent = useRef<HTMLDivElement>(null);
  const rows = useVirtualizer({
    count: assets.length,
    getScrollElement: () => parent.current,
    estimateSize: () => 68,
    overscan: 3,
  });
  const bad = assets.filter((asset) => asset.issue).length;
  return (
    <article
      className={`vs-import ${over ? "is-over" : ""}`}
      data-drop-group={group}
      aria-label={`视频组 ${group.toUpperCase()}`}
    >
      <div className="term-group-head">
        <h2>
          <span className="term-accent">{group.toUpperCase()}/</span> 视频组{" "}
          {group.toUpperCase()}
        </h2>
        <button
          className="vs-text-button"
          disabled={locked || busy || !assets.length}
          onClick={() => onRemove()}
        >
          清空
        </button>
      </div>
      <div
        className="vs-file-list"
        ref={parent}
        style={{ height: Math.min(assets.length * 68, 250) }}
      >
        <div style={{ height: rows.getTotalSize(), position: "relative" }}>
          {rows.getVirtualItems().map((row) => {
            const asset = assets[row.index];
            if (!asset) return null;
            return (
              <div
                key={asset.id}
                className="vs-file"
                data-index={row.index}
                ref={rows.measureElement}
                style={{
                  position: "absolute",
                  width: "100%",
                  top: 0,
                  transform: `translateY(${row.start}px)`,
                }}
              >
                <span className="vs-file-icon" aria-hidden="true">
                  ├─
                </span>
                <div className="vs-file-info">
                  <span className="vs-file-name" title={asset.path}>
                    {asset.name}
                  </span>
                  <span
                    className={`vs-file-meta ${asset.issue ? "vs-file-error" : ""}`}
                    title={asset.issue ?? undefined}
                  >
                    {asset.issue ??
                      `${duration(asset.seconds)} · ${bytes(asset.bytes)} · ${asset.width} × ${asset.height}`}
                  </span>
                </div>
                <button
                  className="vs-text-button"
                  disabled={busy || active}
                  onClick={() => onRelink(asset.id)}
                  title="重新定位并重新检查源文件"
                >
                  定位
                </button>
                <button
                  className="vs-icon-button"
                  disabled={locked || busy}
                  onClick={() => onRemove(asset.id)}
                  aria-label={`移除 ${asset.name}`}
                >
                  ×
                </button>
              </div>
            );
          })}
        </div>
      </div>
      <div className={`vs-drop ${locked ? "is-locked" : ""}`}>
        <span className="term-drop-prompt" aria-hidden="true">
          └─
        </span>
        <div>
          <button
            className="vs-file-button"
            disabled={locked || busy}
            onClick={onAdd}
          >
            + 添加视频
          </button>
          <p>{busy ? "正在处理操作…" : "拖拽至此 / 支持多选 · MP4、MOV"}</p>
        </div>
      </div>
      <div className="vs-group-summary">
        {assets.length
          ? `${assets.length} 个视频 · ${bytes(assets.reduce((sum, a) => sum + a.bytes, 0))}${bad ? ` · ${bad} 个异常` : ""}`
          : "尚未添加视频"}
      </div>
    </article>
  );
}
