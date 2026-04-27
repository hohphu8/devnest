import {
  type KeyboardEvent as ReactKeyboardEvent,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { runAsyncAction } from "@/app/store/async-action-store";
import { useToastStore } from "@/app/store/toast-store";
import { useProjectStore } from "@/app/store/project-store";
import { useServiceStore } from "@/app/store/service-store";
import { useWorkspaceStore } from "@/app/store/workspace-store";
import { Icon } from "@/components/ui/icons";
import { projectProfileApi } from "@/lib/api/project-profile-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type { ServiceName } from "@/types/service";

type PaletteGroup = "navigation" | "commands" | "projects" | "logs";

interface PaletteItem {
  id: string;
  title: string;
  subtitle: string;
  group: PaletteGroup;
  keywords: string;
  actionKey?: string;
  busyLabel?: string;
  perform: () => Promise<void> | void;
}

const NAV_ITEMS = [
  { to: "/", label: "Dashboard", hint: "Workspace overview", keywords: "home metrics workspace" },
  { to: "/projects", label: "Projects", hint: "Project-first registry", keywords: "sites apps import" },
  { to: "/recipes", label: "Recipes", hint: "Scaffold or clone", keywords: "laravel wordpress git" },
  { to: "/services", label: "Services", hint: "Runtime control", keywords: "apache nginx frankenphp mysql redis mailpit" },
  { to: "/logs", label: "Logs", hint: "Tail runtime output", keywords: "viewer output errors" },
  { to: "/diagnostics", label: "Diagnostics", hint: "Project health checks", keywords: "issues health checks" },
  { to: "/reliability", label: "Reliability", hint: "Recovery and preflight", keywords: "repair backup restore" },
  { to: "/databases", label: "Databases", hint: "Create, drop, link", keywords: "mysql mariadb sql" },
  { to: "/settings", label: "Settings", hint: "Runtimes and optional tools", keywords: "php web server tunnel" },
] as const;

const GROUP_LABELS: Record<PaletteGroup, string> = {
  navigation: "Navigation",
  commands: "Commands",
  projects: "Projects",
  logs: "Logs",
};

function serviceLabel(name: ServiceName) {
  switch (name) {
    case "apache":
      return "Apache";
    case "nginx":
      return "Nginx";
    case "frankenphp":
      return "FrankenPHP";
    case "mysql":
      return "MySQL";
    case "mailpit":
      return "Mailpit";
    case "redis":
      return "Redis";
  }
}

function isMacPlatform() {
  if (typeof navigator === "undefined") {
    return false;
  }

  const navigatorWithUserAgentData = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  const platform =
    navigatorWithUserAgentData.userAgentData?.platform ??
    navigator.platform ??
    navigator.userAgent;
  return /mac/i.test(platform);
}

function getItemScore(item: PaletteItem, query: string) {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    switch (item.group) {
      case "projects":
        return 1200;
      case "commands":
        return 850;
      case "navigation":
        return 650;
      case "logs":
        return 450;
    }
  }

  const title = item.title.toLowerCase();
  const subtitle = item.subtitle.toLowerCase();
  const keywords = item.keywords.toLowerCase();
  const haystack = `${title} ${subtitle} ${keywords}`;
  const tokens = normalizedQuery.split(/\s+/).filter(Boolean);

  if (!tokens.every((token) => haystack.includes(token))) {
    return Number.NEGATIVE_INFINITY;
  }

  let score =
    item.group === "projects"
      ? normalizedQuery.length <= 2
        ? 900
        : 500
      : item.group === "commands"
        ? 320
        : item.group === "navigation"
          ? 240
          : 120;

  if (title === normalizedQuery) {
    score += 1600;
  }
  if (subtitle === normalizedQuery) {
    score += 1200;
  }
  if (title.startsWith(normalizedQuery)) {
    score += 1100;
  }
  if (subtitle.startsWith(normalizedQuery)) {
    score += item.group === "projects" ? 900 : 420;
  }
  if (keywords.startsWith(normalizedQuery)) {
    score += item.group === "projects" ? 700 : 280;
  }
  if (title.includes(normalizedQuery)) {
    score += 650;
  }
  if (subtitle.includes(normalizedQuery)) {
    score += item.group === "projects" ? 420 : 220;
  }
  if (keywords.includes(normalizedQuery)) {
    score += 140;
  }

  for (const token of tokens) {
    if (title.startsWith(token)) {
      score += 140;
    }
    if (subtitle.startsWith(token)) {
      score += item.group === "projects" ? 120 : 70;
    }
    if (keywords.includes(token)) {
      score += 40;
    }
  }

  return score;
}

export function GlobalCommandPalette() {
  const navigate = useNavigate();
  const location = useLocation();
  const pushToast = useToastStore((state) => state.push);
  const projects = useProjectStore((state) => state.projects);
  const services = useServiceStore((state) => state.services);
  const startService = useServiceStore((state) => state.startService);
  const stopService = useServiceStore((state) => state.stopService);
  const restartService = useServiceStore((state) => state.restartService);
  const refreshOverview = useWorkspaceStore((state) => state.refreshOverview);
  const workspaceLoading = useWorkspaceStore((state) => state.loading);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const deferredQuery = useDeferredValue(query.trim().toLowerCase());
  const shortcutLabel = isMacPlatform() ? "Cmd+K" : "Ctrl+K";

  function dismissPalette() {
    setOpen(false);
    setQuery("");
    setActiveIndex(0);
  }

  useEffect(() => {
    if (!open) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    }, 0);

    return () => window.clearTimeout(timeoutId);
  }, [open]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || event.repeat) {
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setOpen(true);
        return;
      }

      if (event.key === "Escape" && open) {
        dismissPalette();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open]);

  const items = useMemo<PaletteItem[]>(() => {
    const navigationItems: PaletteItem[] = NAV_ITEMS.map((item) => ({
      id: `nav:${item.to}`,
      title: item.label,
      subtitle: item.hint,
      group: "navigation",
      keywords: item.keywords,
      perform: () => navigate(item.to),
    }));

    const commandItems: PaletteItem[] = [
      {
        id: "command:add-project",
        title: "Add Project",
        subtitle: "Open Smart Scan and project import wizard",
        group: "commands",
        keywords: "import wizard create scan new site",
        actionKey: "palette:add-project",
        busyLabel: "Opening project wizard...",
        perform: () => navigate("/projects?wizard=1"),
      },
      {
        id: "command:import-project-profile",
        title: "Import Project Profile",
        subtitle: "Load a local .devnest-project.json profile into this workspace",
        group: "commands",
        keywords: "json profile import backup",
        actionKey: "palette:import-project-profile",
        busyLabel: "Importing project profile...",
        perform: async () => {
          const result = await projectProfileApi.importProject();
          if (!result) {
            return;
          }

          await refreshOverview();
          navigate(`/projects?projectId=${result.project.id}`);
          pushToast({
            tone: "success",
            title: "Profile imported",
            message: `${result.project.name} was imported into the local project registry.`,
          });
        },
      },
      {
        id: "command:import-team-project-profile",
        title: "Import Team Project",
        subtitle: "Import a team-share profile and choose a local project folder",
        group: "commands",
        keywords: "team share handoff json import",
        actionKey: "palette:import-team-project",
        busyLabel: "Importing team project...",
        perform: async () => {
          const result = await projectProfileApi.importTeamProject();
          if (!result) {
            return;
          }

          await refreshOverview();
          navigate(`/projects?projectId=${result.project.id}`);
          pushToast({
            tone: result.warnings.length > 0 ? "warning" : "success",
            title: "Team project imported",
            message:
              result.warnings.length > 0
                ? `${result.project.name} is now tracked with ${result.warnings.length} compatibility warning(s).`
                : `${result.project.name} is now tracked on this machine.`,
          });
        },
      },
      {
        id: "command:refresh-workspace",
        title: "Refresh Workspace",
        subtitle: "Reload project and service state from native commands",
        group: "commands",
        keywords: "reload refresh services projects state",
        actionKey: "palette:refresh-workspace",
        busyLabel: "Refreshing workspace...",
        perform: async () => {
          await refreshOverview();
          pushToast({
            tone: "success",
            title: "Workspace refreshed",
            message: "Projects and services were reloaded.",
          });
        },
      },
    ];

    const projectItems: PaletteItem[] = projects.map((project) => ({
      id: `project:${project.id}`,
      title: project.name,
      subtitle: `${project.domain} · ${project.framework} · ${project.serverType} · PHP ${project.phpVersion}`,
      group: "projects",
      keywords: `${project.path} ${project.domain} ${project.framework} ${project.serverType} php ${project.phpVersion}`,
      perform: () => navigate(`/projects?projectId=${project.id}`),
    }));

    const logItems: PaletteItem[] = services.map((service) => ({
      id: `logs:${service.name}`,
      title: `Logs: ${serviceLabel(service.name)}`,
      subtitle: `Open ${serviceLabel(service.name)} log tail`,
      group: "logs",
      keywords: `${service.name} logs output runtime tail`,
      perform: () => navigate(`/logs?source=${service.name}`),
    }));

    const serviceCommandItems: PaletteItem[] = services.flatMap((service) => {
      const label = serviceLabel(service.name);
      const nextItems: PaletteItem[] = [];

      if (service.status === "running") {
        nextItems.push({
          id: `command:restart:${service.name}`,
          title: `Restart ${label}`,
          subtitle: `Restart the ${label} managed service`,
          group: "commands",
          keywords: `${service.name} restart service runtime`,
          perform: async () => {
            await restartService(service.name);
            pushToast({
              tone: "success",
              title: "Service restarted",
              message: `${label} was restarted.`,
            });
          },
        });
        nextItems.push({
          id: `command:stop:${service.name}`,
          title: `Stop ${label}`,
          subtitle: `Stop the ${label} managed service`,
          group: "commands",
          keywords: `${service.name} stop service runtime`,
          perform: async () => {
            await stopService(service.name);
            pushToast({
              tone: "success",
              title: "Service stopped",
              message: `${label} was stopped.`,
            });
          },
        });
      } else {
        nextItems.push({
          id: `command:start:${service.name}`,
          title: `Start ${label}`,
          subtitle: `Start the ${label} managed service`,
          group: "commands",
          keywords: `${service.name} start service runtime`,
          perform: async () => {
            await startService(service.name);
            pushToast({
              tone: "success",
              title: "Service started",
              message: `${label} is running.`,
            });
          },
        });
      }

      return nextItems;
    });

    return [
      ...navigationItems,
      ...commandItems,
      ...serviceCommandItems,
      ...projectItems,
      ...logItems,
    ];
  }, [
    navigate,
    projects,
    pushToast,
    refreshOverview,
    restartService,
    services,
    startService,
    stopService,
  ]);

  const { visibleItems, groupOrder } = useMemo(() => {
    const grouped = {
      navigation: [] as Array<{ item: PaletteItem; score: number }>,
      commands: [] as Array<{ item: PaletteItem; score: number }>,
      projects: [] as Array<{ item: PaletteItem; score: number }>,
      logs: [] as Array<{ item: PaletteItem; score: number }>,
    };
    let topProjectScore = Number.NEGATIVE_INFINITY;

    for (const item of items) {
      const score = getItemScore(item, deferredQuery);
      if (score <= Number.NEGATIVE_INFINITY) {
        continue;
      }
      grouped[item.group].push({ item, score });
      if (item.group === "projects") {
        topProjectScore = Math.max(topProjectScore, score);
      }
    }

    const normalized = {
      navigation: grouped.navigation
        .sort((left, right) => right.score - left.score)
        .map(({ item }) => item),
      commands: grouped.commands
        .sort((left, right) => right.score - left.score)
        .map(({ item }) => item),
      projects: grouped.projects
        .sort((left, right) => right.score - left.score)
        .map(({ item }) => item),
      logs: grouped.logs
        .sort((left, right) => right.score - left.score)
        .map(({ item }) => item),
    };

    if (!deferredQuery) {
      normalized.projects = normalized.projects.slice(0, 6);
      normalized.commands = normalized.commands.slice(0, 8);
      normalized.logs = normalized.logs.slice(0, 5);
    }

    const prioritizeProjects =
      deferredQuery.length <= 2 || topProjectScore >= 1000;

    return {
      visibleItems: normalized,
      groupOrder: prioritizeProjects
        ? (["projects", "commands", "navigation", "logs"] satisfies PaletteGroup[])
        : (["navigation", "commands", "projects", "logs"] satisfies PaletteGroup[]),
    };
  }, [deferredQuery, items]);

  const orderedItems = useMemo(
    () => groupOrder.flatMap((group) => visibleItems[group]),
    [groupOrder, visibleItems],
  );

  useEffect(() => {
    setActiveIndex(0);
  }, [deferredQuery, open]);

  useEffect(() => {
    if (orderedItems.length === 0) {
      setActiveIndex(0);
      return;
    }

    if (activeIndex >= orderedItems.length) {
      setActiveIndex(orderedItems.length - 1);
    }
  }, [activeIndex, orderedItems.length]);

  async function runItem(item: PaletteItem) {
    try {
      const perform = async () => {
        dismissPalette();
        await item.perform();
      };

      if (item.actionKey) {
        await runAsyncAction(item.actionKey, perform, item.busyLabel);
      } else {
        await perform();
      }
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Command failed",
        message: getAppErrorMessage(error, `Could not run "${item.title}".`),
      });
    }
  }

  function handleInputKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (orderedItems.length === 0) {
        return;
      }
      setActiveIndex((current) => (current + 1) % orderedItems.length);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (orderedItems.length === 0) {
        return;
      }
      setActiveIndex((current) => (current - 1 + orderedItems.length) % orderedItems.length);
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      const selected = orderedItems[activeIndex];
      if (selected) {
        void runItem(selected);
      }
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      dismissPalette();
    }
  }

  return (
    <>
      <button
        aria-expanded={open}
        aria-haspopup="dialog"
        className="title-search title-search-trigger"
        onClick={() => setOpen(true)}
        type="button"
      >
        <span className="title-search-leading" aria-hidden="true">
          <Icon name="search" />
        </span>
        <span className="title-search-trigger-copy">
          <span className="title-search-trigger-title">Search projects, logs, commands</span>
          <span className="title-search-trigger-meta">
            Type a project name, then press Enter
          </span>
        </span>
        <span className="title-search-shortcut">{shortcutLabel}</span>
      </button>

      {open ? (
        <div
          className="command-palette-overlay"
          onClick={dismissPalette}
          role="presentation"
        >
          <div
            aria-label="Global search and command launcher"
            aria-modal="true"
            className="command-palette"
            onClick={(event) => event.stopPropagation()}
            role="dialog"
          >
            <div className="command-palette-head">
              <input
                aria-label="Search projects, logs, commands"
                className="command-palette-search"
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={handleInputKeyDown}
                placeholder="Search projects first, then commands or logs..."
                ref={inputRef}
                value={query}
              />
              <span className="title-search-shortcut">Esc</span>
            </div>

            <div className="command-palette-results">
              {workspaceLoading ? (
                <div className="command-palette-state">
                  <strong>Loading projects and commands...</strong>
                  <span>DevNest is pulling workspace context so project hits can surface first.</span>
                </div>
              ) : null}

              {orderedItems.length === 0 ? (
                <div className="command-palette-state">
                  <strong>No matches found</strong>
                  <span>Try a project name, local domain, or command words like import, restart, settings, or logs.</span>
                </div>
              ) : (
                groupOrder.map((group) => {
                  const groupItems = visibleItems[group];
                  if (groupItems.length === 0) {
                    return null;
                  }

                  return (
                    <section className="command-palette-section" key={group}>
                      <div className="command-palette-section-title">{GROUP_LABELS[group]}</div>
                      <div className="command-palette-list">
                        {groupItems.map((item) => {
                          const index = orderedItems.findIndex((candidate) => candidate.id === item.id);
                          const active = index === activeIndex;
                          const isCurrentNav =
                            group === "navigation" &&
                            item.id === `nav:${location.pathname === "/" ? "/" : location.pathname}`;
                          return (
                            <button
                              className="command-palette-item"
                              data-active={active}
                              key={item.id}
                              onClick={() => void runItem(item)}
                              onMouseEnter={() => setActiveIndex(index)}
                              type="button"
                            >
                              <span className="command-palette-item-copy">
                                <strong>{item.title}</strong>
                                <span>{item.subtitle}</span>
                              </span>
                              {isCurrentNav ? (
                                <span className="status-chip" data-tone="success">
                                  current
                                </span>
                              ) : null}
                            </button>
                          );
                        })}
                      </div>
                    </section>
                  );
                })
              )}
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
