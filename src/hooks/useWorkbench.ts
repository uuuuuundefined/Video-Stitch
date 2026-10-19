import { useCallback, useEffect, useRef, useState } from "react";
import { api, desktopAvailable, errorMessage } from "../lib/api";
import type { EncoderStatus, Performance, Snapshot } from "../lib/contracts";

// 任务页约 2Hz、性能约 1Hz 拉取；请求互斥，长队列只传当前页。
export function useWorkbench() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [environment, setEnvironment] = useState<EncoderStatus | null>(null);
  const [performance, setPerformance] = useState<Performance | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [page, setPage] = useState(0);
  const pageRef = useRef(page);
  pageRef.current = page;
  const polling = useRef(false);
  const actionPending = useRef(false);

  const refresh = useCallback(async () => {
    if (!desktopAvailable || polling.current) return;
    polling.current = true;
    try {
      const requestedPage = pageRef.current;
      const [next, status] = await Promise.all([
        api.snapshot(requestedPage),
        api.environment(),
      ]);
      setSnapshot(next);
      if (pageRef.current === requestedPage && next.page !== requestedPage)
        setPage(next.page);
      setEnvironment(status);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      polling.current = false;
    }
  }, []);

  useEffect(() => {
    if (!desktopAvailable) return;
    void refresh();
    const stateTimer = window.setInterval(() => void refresh(), 500);
    let pending = false;
    const sample = async () => {
      if (pending) return;
      pending = true;
      try {
        setPerformance(await api.performance());
      } catch {
        setPerformance(null);
      } finally {
        pending = false;
      }
    };
    void sample();
    const metricTimer = window.setInterval(() => void sample(), 1000);
    return () => {
      clearInterval(stateTimer);
      clearInterval(metricTimer);
    };
  }, [refresh]);

  useEffect(() => {
    void refresh();
  }, [page, refresh]);

  const run = useCallback(
    async (action: () => Promise<void>) => {
      if (actionPending.current) return;
      actionPending.current = true;
      setBusy(true);
      setError(null);
      try {
        await action();
      } catch (error) {
        setError(errorMessage(error));
      } finally {
        actionPending.current = false;
        setBusy(false);
        await refresh();
      }
    },
    [refresh],
  );

  return {
    snapshot,
    environment,
    performance,
    error,
    busy,
    page,
    setPage,
    run,
    refresh,
    setError,
  };
}
