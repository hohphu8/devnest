import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppStore } from "@/app/store/app-store";
import { useProjectStore } from "@/app/store/project-store";
import { useServiceStore } from "@/app/store/service-store";
import { GlobalCommandPalette } from "@/components/layout/global-command-palette";
import { Icon } from "@/components/ui/icons";

function hasTauriWindow(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function TitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);
  const sidebarCollapsed = useAppStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useAppStore((state) => state.toggleSidebar);
  const projectsLoaded = useProjectStore((state) => state.loaded);
  const projectsLoading = useProjectStore((state) => state.loading);
  const projectError = useProjectStore((state) => state.error);
  const servicesLoaded = useServiceStore((state) => state.loaded);
  const servicesLoading = useServiceStore((state) => state.loading);
  const serviceError = useServiceStore((state) => state.error);

  const workspaceLoading =
    !projectsLoaded || !servicesLoaded || projectsLoading || servicesLoading;
  const workspaceError = projectError ?? serviceError;
  const workspaceTone = workspaceError ? "error" : workspaceLoading ? "warning" : "success";
  const workspaceLabel = workspaceError
    ? "Needs review"
    : workspaceLoading
      ? "Loading workspace"
      : "Ready";

  useEffect(() => {
    if (!hasTauriWindow()) {
      return;
    }

    const appWindow = getCurrentWindow();
    void appWindow.isMaximized().then(setIsMaximized).catch(() => setIsMaximized(false));
  }, []);

  async function handleStartDragging() {
    if (!hasTauriWindow()) {
      return;
    }

    try {
      await getCurrentWindow().startDragging();
    } catch {
      // Ignore drag errors outside the native runtime.
    }
  }

  async function handleMinimize() {
    if (!hasTauriWindow()) {
      return;
    }

    await getCurrentWindow().minimize();
  }

  async function handleToggleMaximize() {
    if (!hasTauriWindow()) {
      return;
    }

    const appWindow = getCurrentWindow();
    await appWindow.toggleMaximize();
    const maximized = await appWindow.isMaximized();
    setIsMaximized(maximized);
  }

  async function handleClose() {
    if (!hasTauriWindow()) {
      return;
    }

    await getCurrentWindow().close();
  }

  return (
    <header className="title-bar">
      <div className="title-brand title-drag-region" onMouseDown={() => void handleStartDragging()} role="presentation">
        <div className="brand-mark">
          <span />
          <span />
          <span />
        </div>
        <div className="brand-copy">
          <strong>DevNest</strong>
          <div className="brand-subtitle">Local PHP Workspace</div>
        </div>
      </div>

      <div className="title-command">
        <GlobalCommandPalette />
      </div>

      <div className="title-actions">
        <div className="title-indicators">
          <button
            aria-label={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
            className="window-control-button title-action-button"
            onClick={toggleSidebar}
            title={sidebarCollapsed ? "Show Sidebar (Ctrl+B)" : "Hide Sidebar (Ctrl+B)"}
            type="button"
          >
            <Icon name="sidebar" />
          </button>
          <span className="status-chip" data-tone={workspaceTone}>
            <Icon name={workspaceError ? "alert" : workspaceLoading ? "activity" : "check"} />
            {workspaceLabel}
          </span>
        </div>
        <div className="window-controls">
          <button
            aria-label="Minimize window"
            className="window-control-button"
            onClick={() => void handleMinimize()}
            type="button"
          >
            <span className="window-control-line" />
          </button>
          <button
            aria-label={isMaximized ? "Restore window" : "Maximize window"}
            className="window-control-button"
            onClick={() => void handleToggleMaximize()}
            type="button"
          >
            <span className={isMaximized ? "window-control-restore" : "window-control-square"} />
          </button>
          <button
            aria-label="Close window"
            className="window-control-button window-control-button-close"
            onClick={() => void handleClose()}
            type="button"
          >
            <span className="window-control-close" />
          </button>
        </div>
      </div>
    </header>
  );
}
