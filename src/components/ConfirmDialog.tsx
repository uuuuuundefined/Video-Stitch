import { useEffect, useRef } from "react";

export interface DialogRequest {
  title: string;
  body: string;
  confirm: string;
  action?: () => void;
}

// 原生 modal 提供焦点约束和 Escape，确认动作只能触发一次。
export function ConfirmDialog({
  request,
  onClose,
}: {
  request: DialogRequest | null;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (request && !ref.current?.open) ref.current?.showModal();
    else if (!request) ref.current?.close();
  }, [request]);
  return (
    <dialog
      className="vs-dialog"
      ref={ref}
      aria-labelledby="dialog-title"
      onCancel={onClose}
      onClose={onClose}
    >
      <form method="dialog">
        <div className="term-dialog-head">
          <h2 id="dialog-title">{request?.title}</h2>
          <button
            className="vs-icon-button"
            aria-label="关闭对话框"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <p>{request?.body}</p>
        <div className="term-dialog-buttons">
          <button className="vs-secondary" onClick={onClose}>
            {request?.action ? "返回" : "关闭"}
          </button>
          {request?.action ? (
            <button
              type="button"
              className="vs-primary"
              onClick={() => {
                const action = request.action;
                onClose();
                action?.();
              }}
            >
              {request.confirm}
            </button>
          ) : null}
        </div>
      </form>
    </dialog>
  );
}
