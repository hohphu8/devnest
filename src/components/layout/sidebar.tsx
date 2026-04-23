import { useAppStore } from "@/app/store/app-store";
import { NavLink, useNavigate } from "react-router-dom";
import { useProjectStore } from "@/app/store/project-store";
import { useWorkspaceStore } from "@/app/store/workspace-store";
import { Icon, type IconName } from "@/components/ui/icons";
import { useServiceStore } from "@/app/store/service-store";
import { cn } from "@/lib/utils";

const primaryNav: Array<{
  to: string;
  label: string;
  icon: IconName;
  hint: string;
}> = [
  { to: "/", label: "Dashboard", icon: "home", hint: "Workspace" },
  { to: "/projects", label: "Projects", icon: "folder", hint: "Sites and apps" },
  { to: "/recipes", label: "Recipes", icon: "recipes", hint: "Create or clone" },
  { to: "/services", label: "Services", icon: "server", hint: "Runtime control" },
  { to: "/workers", label: "Workers", icon: "activity", hint: "Task and cron monitor" },
  { to: "/tasks", label: "Tasks", icon: "clock", hint: "Scheduled automation" },
  { to: "/logs", label: "Logs", icon: "logs", hint: "Live output" },
  { to: "/diagnostics", label: "Diagnostics", icon: "diagnostics", hint: "Health checks" },
  { to: "/reliability", label: "Reliability", icon: "reliability", hint: "Repair and recovery" },
  { to: "/databases", label: "Databases", icon: "database", hint: "Local MySQL" },
  { to: "/settings", label: "Settings", icon: "settings", hint: "Runtimes and defaults" },
];

const quickActions: Array<{
  action: "import" | "refresh";
  icon: IconName;
  label: string;
  hint: string;
}> = [
  {
    action: "import",
    icon: "plus",
    label: "Import Project",
    hint: "Open Smart Scan",
  },
  {
    action: "refresh",
    icon: "refresh",
    label: "Refresh",
    hint: "Reload workspace",
  },
];

export function Sidebar() {
  const navigate = useNavigate();
  const sidebarCollapsed = useAppStore((state) => state.sidebarCollapsed);
  const projects = useProjectStore((state) => state.projects);
  const refreshOverview = useWorkspaceStore((state) => state.refreshOverview);
  const services = useServiceStore((state) => state.services);

  const recentProjects = projects.slice(0, 3);
  const runningServices = services.filter((service) => service.status === "running").length;
  const serviceErrors = services.filter((service) => service.status === "error").length;

  return (
    <aside
      aria-label="Primary sidebar"
      className="sidebar"
      data-collapsed={sidebarCollapsed}
    >
      <div className="sidebar-panel">
        <div className="sidebar-section-title">{sidebarCollapsed ? "Nav" : "Navigation"}</div>
        <nav className="nav-list">
          {primaryNav.map((item) => (
            <NavLink
              aria-label={item.label}
              key={item.to}
              className={({ isActive }) => cn("nav-item", isActive && "active")}
              title={item.label}
              to={item.to}
            >
              <span className="nav-glyph" aria-hidden="true">
                <Icon name={item.icon} />
              </span>
              <span className="nav-copy">
                <strong>{item.label}</strong>
                <span>{item.hint}</span>
              </span>
            </NavLink>
          ))}
        </nav>
      </div>

      <div className="sidebar-panel">
        <div className="sidebar-section-title">{sidebarCollapsed ? "Quick" : "Quick Actions"}</div>
        <div className="nav-list">
          {quickActions.map((item) => (
            <button
              aria-label={item.label}
              className="nav-item nav-item-muted"
              key={item.label}
              onClick={() => {
                if (item.action === "import") {
                  navigate("/projects?wizard=1");
                  return;
                }

                void refreshOverview().catch(() => undefined);
              }}
              style={{ textAlign: "left" }}
              title={item.label}
              type="button"
            >
              <span className="nav-glyph" aria-hidden="true">
                <Icon name={item.icon} />
              </span>
              <span className="nav-copy">
                <strong>{item.label}</strong>
                <span>{item.hint}</span>
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="sidebar-panel">
        <div className="sidebar-section-title">{sidebarCollapsed ? "Recent" : "Recent Projects"}</div>
        <div className="nav-list">
          {recentProjects.length > 0 ? (
            recentProjects.map((project) => (
              <button
                aria-label={project.name}
                className="nav-item nav-item-muted"
                key={project.id}
                onClick={() => navigate(`/projects?projectId=${project.id}`)}
                style={{ textAlign: "left" }}
                title={project.name}
                type="button"
              >
                <span className="nav-glyph" aria-hidden="true">
                  <Icon name="folderOpen" />
                </span>
                <span className="nav-copy">
                  <strong>{project.name}</strong>
                  <span>{project.framework} / {project.serverType}</span>
                </span>
              </button>
            ))
          ) : (
            <div className="detail-item">
              <span className="helper-text">Import a project to build a recent list.</span>
            </div>
          )}
        </div>
      </div>
    </aside>
  );
}
