import { useEffect, type PropsWithChildren } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { useAppStore } from "@/app/store/app-store";
import { Sidebar } from "@/components/layout/sidebar";
import { StatusStrip } from "@/components/layout/status-strip";
import { TitleBar } from "@/components/layout/title-bar";
import { ToastViewport } from "@/components/ui/toast-viewport";
import type { ServiceName } from "@/types/service";

const TRAY_LOGS_NAVIGATION_EVENT = "devnest:navigate-logs";
const SERVICE_NAMES: ServiceName[] = [
  "apache",
  "nginx",
  "frankenphp",
  "mysql",
  "mailpit",
  "redis",
];

interface TrayLogsNavigationPayload {
  source?: string;
}

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function isServiceName(value: string | undefined): value is ServiceName {
  return SERVICE_NAMES.includes(value as ServiceName);
}

export function AppShell({ children }: PropsWithChildren) {
  const navigate = useNavigate();
  const sidebarCollapsed = useAppStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useAppStore((state) => state.toggleSidebar);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen<TrayLogsNavigationPayload>(TRAY_LOGS_NAVIGATION_EVENT, (event) => {
      if (!isServiceName(event.payload.source)) {
        return;
      }

      navigate(`/logs?type=service&source=${encodeURIComponent(event.payload.source)}`);
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
  }, [navigate]);

  useEffect(() => {
    function handleKeydown(event: KeyboardEvent) {
      const target = event.target;
      const isEditable =
        target instanceof HTMLElement &&
        (target.isContentEditable ||
          target instanceof HTMLInputElement ||
          target instanceof HTMLTextAreaElement ||
          target instanceof HTMLSelectElement);

      if (
        isEditable ||
        event.defaultPrevented ||
        !event.ctrlKey ||
        event.altKey ||
        event.shiftKey ||
        event.key.toLowerCase() !== "b"
      ) {
        return;
      }

      event.preventDefault();
      toggleSidebar();
    }

    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  }, [toggleSidebar]);

  return (
    <div className="app-shell">
      <div className="app-shell-backdrop" />
      <div className="app-frame">
        <TitleBar />
        <div className="shell-body" data-sidebar-collapsed={sidebarCollapsed}>
          <Sidebar />
          <main className="main-panel">
            <div className="main-panel-inner">{children}</div>
          </main>
        </div>
        <StatusStrip />
        <ToastViewport />
      </div>
    </div>
  );
}
