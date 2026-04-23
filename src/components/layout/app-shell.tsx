import { useEffect, type PropsWithChildren } from "react";
import { useAppStore } from "@/app/store/app-store";
import { Sidebar } from "@/components/layout/sidebar";
import { StatusStrip } from "@/components/layout/status-strip";
import { TitleBar } from "@/components/layout/title-bar";
import { ToastViewport } from "@/components/ui/toast-viewport";

export function AppShell({ children }: PropsWithChildren) {
  const sidebarCollapsed = useAppStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useAppStore((state) => state.toggleSidebar);

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
