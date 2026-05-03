import { useEffect, useRef, type PropsWithChildren } from "react";
import { listen } from "@tauri-apps/api/event";
import { useWorkspaceStore } from "@/app/store/workspace-store";

const BOOT_BACKGROUND_COMPLETE_EVENT = "devnest:boot-background-complete";

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function AppProviders({ children }: PropsWithChildren) {
  const bootSettleFallbackRef = useRef<number | undefined>(undefined);
  const bootCompleteRefreshHandledRef = useRef(false);
  const workspaceLoaded = useWorkspaceStore((state) => state.loaded);
  const loadOverview = useWorkspaceStore((state) => state.loadOverview);
  const refreshOverview = useWorkspaceStore((state) => state.refreshOverview);
  const loadPortSummary = useWorkspaceStore((state) => state.loadPortSummary);

  useEffect(() => {
    if (!workspaceLoaded) {
      void loadOverview().catch(() => undefined);
    }
  }, [loadOverview, workspaceLoaded]);

  useEffect(() => {
    if (workspaceLoaded) {
      void loadPortSummary().catch(() => undefined);
    }
  }, [loadPortSummary, workspaceLoaded]);

  useEffect(() => {
    if (!workspaceLoaded || !hasTauriRuntime()) {
      return;
    }

    if (bootCompleteRefreshHandledRef.current) {
      return;
    }

    bootSettleFallbackRef.current = window.setTimeout(() => {
      if (!bootCompleteRefreshHandledRef.current) {
        bootCompleteRefreshHandledRef.current = true;
        void refreshOverview({ silent: true })
          .then(() => loadPortSummary())
          .catch(() => undefined);
      }
    }, 5500);

    return () => {
      if (bootSettleFallbackRef.current !== undefined) {
        window.clearTimeout(bootSettleFallbackRef.current);
        bootSettleFallbackRef.current = undefined;
      }
    };
  }, [loadPortSummary, refreshOverview, workspaceLoaded]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen(BOOT_BACKGROUND_COMPLETE_EVENT, () => {
      if (bootCompleteRefreshHandledRef.current) {
        return;
      }

      bootCompleteRefreshHandledRef.current = true;
      if (bootSettleFallbackRef.current !== undefined) {
        window.clearTimeout(bootSettleFallbackRef.current);
        bootSettleFallbackRef.current = undefined;
      }

      const workspace = useWorkspaceStore.getState();
      const refreshAfterInitialLoad = () => {
        void refreshOverview({ silent: true })
          .then(() => loadPortSummary())
          .catch(() => undefined);
      };

      if (!workspace.loaded || workspace.loading) {
        void workspace.loadOverview().finally(refreshAfterInitialLoad);
        return;
      }

      refreshAfterInitialLoad();
    })
      .then((dispose) => {
        if (disposed) {
          dispose();
          return;
        }

        unlisten = dispose;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadPortSummary, refreshOverview]);

  return children;
}
