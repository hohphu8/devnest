import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useToastStore } from "@/app/store/toast-store";
import { useProjectStore } from "@/app/store/project-store";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { StickyTabs } from "@/components/ui/sticky-tabs";
import { projectApi } from "@/lib/api/project-api";
import { recipeApi } from "@/lib/api/recipe-api";
import { runtimeApi } from "@/lib/api/runtime-api";
import { getAppErrorMessage } from "@/lib/tauri";
import { installedPhpVersionFamilies } from "@/lib/runtime-version";
import {
  domainSchema,
  gitRepositoryUrlSchema,
  recipeTargetPathSchema,
} from "@/lib/validators";
import type { ServerType } from "@/types/project";
import type { RuntimeInventoryItem } from "@/types/runtime";

interface RecipeBaseForm {
  path: string;
  domain: string;
  phpVersion: string;
  serverType: ServerType;
  sslEnabled: boolean;
}

type RecipeStudioTab = "presets" | "laravel" | "wordpress" | "git";
type RecipeFormTab = Exclude<RecipeStudioTab, "presets">;

interface GitRecipeForm extends RecipeBaseForm {
  repositoryUrl: string;
  branch: string;
}

interface RecipePreset {
  id: string;
  recipe: "laravel" | "wordpress" | "git";
  title: string;
  description: string;
  helper: string;
  defaults: Partial<RecipeBaseForm & Pick<GitRecipeForm, "repositoryUrl" | "branch">>;
}

const defaultBaseForm: RecipeBaseForm = {
  path: "",
  domain: "",
  phpVersion: "8.4",
  serverType: "apache",
  sslEnabled: true,
};

const defaultGitForm: GitRecipeForm = {
  ...defaultBaseForm,
  repositoryUrl: "",
  branch: "",
};

function inferDomainFromPath(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  const tail = (segments.length > 0 ? segments[segments.length - 1] : "project").toLowerCase();
  const normalized = tail.replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, "");
  return `${normalized || "project"}.test`;
}

function pathTail(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments.length > 0 ? segments[segments.length - 1] ?? "" : "";
}

function sanitizeFolderName(value: string): string {
  return value
    .trim()
    .replace(/\.git$/i, "")
    .replace(/[<>:"/\\|?*]+/g, "-")
    .replace(/\s+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/[. ]+$/g, "");
}

function folderNameFromDomain(domain: string): string {
  return sanitizeFolderName((domain.split(".")[0] ?? "").toLowerCase());
}

function folderNameFromRepository(repositoryUrl: string): string {
  const normalized = repositoryUrl.trim().replace(/[?#].*$/u, "").replace(/[\\/]+$/u, "");
  const tail = normalized.split(/[\\/]/).filter(Boolean).pop() ?? "";
  return sanitizeFolderName(tail);
}

function joinRecipePath(parentPath: string, folderName: string): string {
  const trimmedParent = parentPath.trim().replace(/[\\/]+$/u, "");
  const separator = trimmedParent.includes("\\") && !trimmedParent.includes("/") ? "\\" : "/";
  return `${trimmedParent}${separator}${folderName}`;
}

function resolveRecipeTargetPath({
  parentPath,
  currentPath,
  domain,
  repositoryUrl,
  fallback,
}: {
  parentPath: string;
  currentPath: string;
  domain: string;
  repositoryUrl?: string;
  fallback: string;
}): string {
  const folderName =
    sanitizeFolderName(pathTail(currentPath)) ||
    folderNameFromDomain(domain) ||
    (repositoryUrl ? folderNameFromRepository(repositoryUrl) : "") ||
    fallback;
  return joinRecipePath(parentPath, folderName);
}

function buildRecipeErrorMessage(form: RecipeBaseForm): string | null {
  const pathCheck = recipeTargetPathSchema.safeParse(form.path);
  if (!pathCheck.success) {
    return pathCheck.error.issues[0]?.message ?? "Recipe target path is required.";
  }

  const domainCheck = domainSchema.safeParse(form.domain);
  if (!domainCheck.success) {
    return domainCheck.error.issues[0]?.message ?? "Domain is invalid.";
  }

  if (!form.phpVersion.trim()) {
    return "Choose a PHP version first.";
  }

  return null;
}

function buildRecipeFailureMessage(error: unknown, fallback: string): string {
  const base = getAppErrorMessage(error, fallback);
  if (
    typeof error === "object" &&
    error !== null &&
    "details" in error &&
    typeof error.details === "string"
  ) {
    const details = error.details.trim();
    if (details.length > 0 && details !== base) {
      return `${base} Details: ${details}`;
    }
  }

  return base;
}

function useDelayedBusy(active: boolean, delayMs = 160) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (!active) {
      setVisible(false);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setVisible(true);
    }, delayMs);

    return () => window.clearTimeout(timeoutId);
  }, [active, delayMs]);

  return visible;
}

function waitForNextPaint() {
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => resolve());
  });
}

function RecipeCard({
  title,
  description,
  helper,
  children,
  action,
}: {
  title: string;
  description: string;
  helper: string;
  children: ReactNode;
  action: ReactNode;
}) {
  return (
    <Card>
      <div className="page-header">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </div>
      <div className="stack">
        {children}
        <span className="helper-text">{helper}</span>
      </div>
      <div className="page-toolbar" style={{ justifyContent: "flex-start", marginTop: 18 }}>
        {action}
      </div>
    </Card>
  );
}

export function RecipeStudio() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [runtimeInventory, setRuntimeInventory] = useState<RuntimeInventoryItem[]>([]);
  const [laravelForm, setLaravelForm] = useState<RecipeBaseForm>(defaultBaseForm);
  const [wordpressForm, setWordPressForm] = useState<RecipeBaseForm>({
    ...defaultBaseForm,
    serverType: "apache",
    sslEnabled: false,
  });
  const [gitForm, setGitForm] = useState<GitRecipeForm>(defaultGitForm);
  const [actionKey, setActionKey] = useState<string | null>(null);
  const [pathPickerKey, setPathPickerKey] = useState<RecipeFormTab | null>(null);
  const [recipeError, setRecipeError] = useState<string | null>(null);
  const navigate = useNavigate();
  const pushToast = useToastStore((state) => state.push);
  const loadProjects = useProjectStore((state) => state.loadProjects);
  const busyOverlayVisible = useDelayedBusy(actionKey !== null);

  useEffect(() => {
    runtimeApi.list().then(setRuntimeInventory).catch(() => setRuntimeInventory([]));
  }, []);

  const phpVersionOptions = useMemo(() => {
    const installed = installedPhpVersionFamilies(runtimeInventory);
    return installed.length > 0 ? installed : ["8.4"];
  }, [runtimeInventory]);
  const defaultPhpVersion = phpVersionOptions[phpVersionOptions.length - 1] ?? "8.4";
  const presets = useMemo<RecipePreset[]>(
    () => [
      {
        id: "laravel-api",
        recipe: "laravel",
        title: "Laravel API",
        description: "Nginx + HTTPS with the latest installed PHP family.",
        helper: "Good default for API-first Laravel apps.",
        defaults: {
          phpVersion: defaultPhpVersion,
          serverType: "nginx",
          sslEnabled: true,
          path: "D:/Sites/laravel-api",
          domain: "laravel-api.test",
        },
      },
      {
        id: "laravel-admin",
        recipe: "laravel",
        title: "Laravel Admin",
        description: "Apache + HTTPS for projects that lean on .htaccess behavior.",
        helper: "Use when Apache parity matters more than raw defaults.",
        defaults: {
          phpVersion: defaultPhpVersion,
          serverType: "apache",
          sslEnabled: true,
          path: "D:/Sites/laravel-admin",
          domain: "laravel-admin.test",
        },
      },
      {
        id: "wordpress-blog",
        recipe: "wordpress",
        title: "WordPress Blog",
        description: "Apache + PHP with a clean WordPress-friendly local profile.",
        helper: "Fast path for a typical content site or marketing blog.",
        defaults: {
          phpVersion: defaultPhpVersion,
          serverType: "apache",
          sslEnabled: false,
          path: "D:/Sites/wp-blog",
          domain: "wp-blog.test",
        },
      },
      {
        id: "wordpress-nginx",
        recipe: "wordpress",
        title: "WordPress on Nginx",
        description: "Nginx + HTTPS for teams standardizing on the Nginx stack.",
        helper: "Keeps the recipe flow but aligns with an Nginx-first workspace.",
        defaults: {
          phpVersion: defaultPhpVersion,
          serverType: "nginx",
          sslEnabled: true,
          path: "D:/Sites/wp-nginx",
          domain: "wp-nginx.test",
        },
      },
      {
        id: "git-starter",
        recipe: "git",
        title: "Git Import",
        description: "Clone + scan + register with HTTPS enabled from day one.",
        helper: "Use for existing repos that should land directly in the tracked project list.",
        defaults: {
          phpVersion: defaultPhpVersion,
          serverType: "nginx",
          sslEnabled: true,
          path: "D:/Sites/shared-repo",
          domain: "shared-repo.test",
          repositoryUrl: "https://github.com/acme/example-php-app.git",
          branch: "main",
        },
      },
      ],
    [defaultPhpVersion],
  );
  const recipeTabs = [
    { id: "presets", label: "Presets", meta: `${presets.length} templates` },
    { id: "laravel", label: "Laravel", meta: "Composer create-project" },
    { id: "wordpress", label: "WordPress", meta: "johnpbloch/wordpress" },
    { id: "git", label: "Git Clone", meta: "clone + scan + register" },
  ] as const;
  const activeTab = (() => {
    const tab = searchParams.get("tab");
    if (tab === "laravel" || tab === "wordpress" || tab === "git") {
      return tab;
    }
    return "presets";
  })();

  function handleSelectTab(tab: RecipeStudioTab) {
    const next = new URLSearchParams(searchParams);
    if (tab === "presets") {
      next.delete("tab");
    } else {
      next.set("tab", tab);
    }
    setSearchParams(next);
  }

  function applyPreset(preset: RecipePreset) {
    if (preset.recipe === "laravel") {
      setLaravelForm((current) => ({
        ...current,
        ...preset.defaults,
      }));
    } else if (preset.recipe === "wordpress") {
      setWordPressForm((current) => ({
        ...current,
        ...preset.defaults,
      }));
    } else {
      setGitForm((current) => ({
        ...current,
        ...preset.defaults,
      }));
    }

    handleSelectTab(preset.recipe);
    pushToast({
      tone: "info",
      title: "Preset applied",
      message: `${preset.title} filled the ${preset.recipe === "git" ? "Git" : preset.recipe === "laravel" ? "Laravel" : "WordPress"} recipe form and opened the matching tab.`,
    });
  }

  async function handlePickTargetPath(tab: RecipeFormTab) {
    setPathPickerKey(tab);
    try {
      const selectedParent = await projectApi.pickFolder();
      if (!selectedParent) {
        return;
      }

      if (tab === "laravel") {
        setLaravelForm((current) => {
          const nextPath = resolveRecipeTargetPath({
            parentPath: selectedParent,
            currentPath: current.path,
            domain: current.domain,
            fallback: "laravel-app",
          });
          return {
            ...current,
            path: nextPath,
            domain: current.domain || inferDomainFromPath(nextPath),
          };
        });
        return;
      }

      if (tab === "wordpress") {
        setWordPressForm((current) => {
          const nextPath = resolveRecipeTargetPath({
            parentPath: selectedParent,
            currentPath: current.path,
            domain: current.domain,
            fallback: "wordpress-site",
          });
          return {
            ...current,
            path: nextPath,
            domain: current.domain || inferDomainFromPath(nextPath),
          };
        });
        return;
      }

      setGitForm((current) => {
        const nextPath = resolveRecipeTargetPath({
          parentPath: selectedParent,
          currentPath: current.path,
          domain: current.domain,
          repositoryUrl: current.repositoryUrl,
          fallback: "git-project",
        });
        return {
          ...current,
          path: nextPath,
          domain: current.domain || inferDomainFromPath(nextPath),
        };
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Folder picker failed",
        message: getAppErrorMessage(error, "Could not open the native folder picker."),
      });
    } finally {
      setPathPickerKey(null);
    }
  }

  async function finalizeRecipe(projectId: string, message: string) {
    await loadProjects();
    pushToast({
      tone: "success",
      title: "Recipe complete",
      message,
    });
    navigate(`/projects?projectId=${projectId}`);
  }

  async function handleLaravelCreate() {
    const validationError = buildRecipeErrorMessage(laravelForm);
    if (validationError) {
      setRecipeError(validationError);
      pushToast({ tone: "error", title: "Laravel recipe blocked", message: validationError });
      return;
    }

    setActionKey("laravel");
    setRecipeError(null);
    await waitForNextPaint();
    try {
      const project = await recipeApi.createLaravel(laravelForm);
      setLaravelForm({
        ...defaultBaseForm,
        phpVersion: laravelForm.phpVersion,
      });
      await finalizeRecipe(project.id, `${project.name} was scaffolded and tracked as a Laravel project.`);
    } catch (error) {
      const nextMessage = buildRecipeFailureMessage(
        error,
        "Could not scaffold the Laravel recipe.",
      );
      setRecipeError(nextMessage);
      pushToast({
        tone: "error",
        title: "Laravel recipe failed",
        message: nextMessage,
      });
    } finally {
      setActionKey(null);
    }
  }

  async function handleWordPressCreate() {
    const validationError = buildRecipeErrorMessage(wordpressForm);
    if (validationError) {
      setRecipeError(validationError);
      pushToast({ tone: "error", title: "WordPress recipe blocked", message: validationError });
      return;
    }

    setActionKey("wordpress");
    setRecipeError(null);
    await waitForNextPaint();
    try {
      const project = await recipeApi.createWordPress(wordpressForm);
      setWordPressForm({
        ...defaultBaseForm,
        serverType: "apache",
        sslEnabled: false,
        phpVersion: wordpressForm.phpVersion,
      });
      await finalizeRecipe(project.id, `${project.name} was scaffolded and tracked as a WordPress project.`);
    } catch (error) {
      const nextMessage = buildRecipeFailureMessage(
        error,
        "Could not scaffold the WordPress recipe.",
      );
      setRecipeError(nextMessage);
      pushToast({
        tone: "error",
        title: "WordPress recipe failed",
        message: nextMessage,
      });
    } finally {
      setActionKey(null);
    }
  }

  async function handleGitClone() {
    const validationError = buildRecipeErrorMessage(gitForm);
    if (validationError) {
      setRecipeError(validationError);
      pushToast({ tone: "error", title: "Git clone blocked", message: validationError });
      return;
    }

    const repositoryCheck = gitRepositoryUrlSchema.safeParse(gitForm.repositoryUrl);
    if (!repositoryCheck.success) {
      const nextMessage =
        repositoryCheck.error.issues[0]?.message ?? "Repository URL is invalid.";
      setRecipeError(nextMessage);
      pushToast({
        tone: "error",
        title: "Git clone blocked",
        message: nextMessage,
      });
      return;
    }

    setActionKey("git");
    setRecipeError(null);
    await waitForNextPaint();
    try {
      const project = await recipeApi.cloneGit({
        ...gitForm,
        branch: gitForm.branch.trim() || null,
      });
      setGitForm({
        ...defaultGitForm,
        phpVersion: gitForm.phpVersion,
      });
      await finalizeRecipe(project.id, `${project.name} was cloned, scanned, and added to DevNest.`);
    } catch (error) {
      const nextMessage = buildRecipeFailureMessage(
        error,
        "Could not clone and register the repository.",
      );
      setRecipeError(nextMessage);
      pushToast({
        tone: "error",
        title: "Git clone failed",
        message: nextMessage,
      });
    } finally {
      setActionKey(null);
    }
  }

  const recipeBusyCopy =
    actionKey === "laravel"
      ? {
          title: "Scaffolding Laravel",
          message: "Composer is creating the Laravel app in the background. DevNest will scan and register it when the scaffold finishes.",
        }
      : actionKey === "wordpress"
        ? {
            title: "Scaffolding WordPress",
            message: "Composer is creating the WordPress app in the background. DevNest will scan and register it when the scaffold finishes.",
          }
        : actionKey === "git"
          ? {
              title: "Cloning Repository",
              message: "Git is cloning the repository in the background. DevNest will scan and register it when the clone finishes.",
            }
          : null;

  return (
    <div className="stack">
      <div className="stack workspace-shell route-loading-shell">
        {busyOverlayVisible && recipeBusyCopy ? (
          <div aria-live="polite" className="loading-scrim" role="status">
            <div className="loading-scrim-card">
              <span aria-hidden="true" className="loading-spinner" />
              <div className="loading-scrim-copy">
                <strong>{recipeBusyCopy.title}</strong>
                <span>{recipeBusyCopy.message}</span>
              </div>
            </div>
          </div>
        ) : null}
        <StickyTabs
          activeTab={activeTab}
          ariaLabel="Recipe Studio sections"
          items={recipeTabs}
          onSelect={handleSelectTab}
        />
        {recipeError ? (
          <div className="inline-note-card" data-tone="error">
            <strong>Recipe action needs attention</strong>
            <span>{recipeError}</span>
          </div>
        ) : null}

        <div
          aria-labelledby="workspace-tab-presets"
          className="workspace-panel"
          hidden={activeTab !== "presets"}
          id="workspace-panel-presets"
          role="tabpanel"
        >
          <Card>
            <div className="page-header">
              <div>
                <h2>Preset Templates</h2>
                <p>Opinionated starting points layered on top of the existing recipe flows. Apply one, tweak if needed, then run the recipe normally.</p>
              </div>
            </div>
            <div className="route-grid" data-columns="3">
              {presets.map((preset) => (
                <div className="detail-item" key={preset.id}>
                  <div className="page-toolbar" style={{ alignItems: "flex-start", justifyContent: "space-between" }}>
                    <div>
                      <strong>{preset.title}</strong>
                      <p style={{ marginTop: 6 }}>{preset.description}</p>
                    </div>
                    <span className="status-chip" data-tone="warning">
                      {preset.recipe}
                    </span>
                  </div>
                  <span className="helper-text">{preset.helper}</span>
                  <div className="page-toolbar" style={{ justifyContent: "flex-start", marginTop: 12 }}>
                    <Button disabled={actionKey !== null} onClick={() => applyPreset(preset)}>
                      Apply Preset
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-laravel"
          className="workspace-panel"
          hidden={activeTab !== "laravel"}
          id="workspace-panel-laravel"
          role="tabpanel"
        >
          <RecipeCard
            action={
              <Button disabled={actionKey !== null} onClick={() => void handleLaravelCreate()} variant="primary">
                {actionKey === "laravel" ? "Creating Laravel..." : "Create Laravel"}
              </Button>
            }
            description="Run Composer create-project, scan the new app, then add it straight into the project registry."
            helper="Requires Composer on the machine. The target folder must not exist yet."
            title="New Laravel App"
          >
          <div className="form-grid">
            <div className="field">
              <label htmlFor="recipe-laravel-path">Target Path</label>
              <div className="recipe-path-field">
                <input
                  className="input mono"
                  id="recipe-laravel-path"
                  onChange={(event) =>
                    setLaravelForm((current) => ({
                      ...current,
                      path: event.target.value,
                      domain: current.domain || inferDomainFromPath(event.target.value),
                    }))
                  }
                  placeholder="D:/Sites/shop-api"
                  value={laravelForm.path}
                />
                <Button
                  disabled={actionKey !== null || pathPickerKey !== null}
                  onClick={() => void handlePickTargetPath("laravel")}
                  variant="ghost"
                >
                  {pathPickerKey === "laravel" ? "Picking..." : "Browse"}
                </Button>
              </div>
              <span className="helper-text">Pick a parent folder. DevNest keeps the project folder name at the end of the target path.</span>
            </div>
            <div className="field">
              <label htmlFor="recipe-laravel-domain">Domain</label>
              <input
                className="input"
                id="recipe-laravel-domain"
                onChange={(event) =>
                  setLaravelForm((current) => ({ ...current, domain: event.target.value }))
                }
                value={laravelForm.domain}
              />
            </div>
            <div className="field">
              <label htmlFor="recipe-laravel-php">PHP Version</label>
              <select
                className="select"
                id="recipe-laravel-php"
                onChange={(event) =>
                  setLaravelForm((current) => ({ ...current, phpVersion: event.target.value }))
                }
                value={laravelForm.phpVersion}
              >
                {phpVersionOptions.map((version) => (
                  <option key={version} value={version}>
                    PHP {version}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label htmlFor="recipe-laravel-server">Web Server</label>
              <select
                className="select"
                id="recipe-laravel-server"
                onChange={(event) =>
                  setLaravelForm((current) => ({
                    ...current,
                    serverType: event.target.value as ServerType,
                  }))
                }
                value={laravelForm.serverType}
              >
                <option value="apache">Apache</option>
                <option value="nginx">Nginx</option>
                <option value="frankenphp">FrankenPHP (Experimental)</option>
              </select>
            </div>
          </div>
          <label className="checkbox-row">
            <input
              checked={laravelForm.sslEnabled}
              onChange={(event) =>
                setLaravelForm((current) => ({ ...current, sslEnabled: event.target.checked }))
              }
              type="checkbox"
            />
            <span>Enable local HTTPS for this project profile</span>
          </label>
          </RecipeCard>
        </div>

        <div
          aria-labelledby="workspace-tab-wordpress"
          className="workspace-panel"
          hidden={activeTab !== "wordpress"}
          id="workspace-panel-wordpress"
          role="tabpanel"
        >
          <RecipeCard
            action={
              <Button
                disabled={actionKey !== null}
                onClick={() => void handleWordPressCreate()}
                variant="primary"
              >
                {actionKey === "wordpress" ? "Creating WordPress..." : "Create WordPress"}
              </Button>
            }
            description="Scaffold a Composer-based WordPress install and track it with sane local defaults."
            helper="Uses the johnpbloch/wordpress package. Apache is the default, but Nginx or FrankenPHP are available if they match the workspace better."
            title="New WordPress Site"
          >
          <div className="form-grid">
            <div className="field">
              <label htmlFor="recipe-wordpress-path">Target Path</label>
              <div className="recipe-path-field">
                <input
                  className="input mono"
                  id="recipe-wordpress-path"
                  onChange={(event) =>
                    setWordPressForm((current) => ({
                      ...current,
                      path: event.target.value,
                      domain: current.domain || inferDomainFromPath(event.target.value),
                    }))
                  }
                  placeholder="D:/Sites/marketing-site"
                  value={wordpressForm.path}
                />
                <Button
                  disabled={actionKey !== null || pathPickerKey !== null}
                  onClick={() => void handlePickTargetPath("wordpress")}
                  variant="ghost"
                >
                  {pathPickerKey === "wordpress" ? "Picking..." : "Browse"}
                </Button>
              </div>
              <span className="helper-text">Pick a parent folder. DevNest keeps the project folder name at the end of the target path.</span>
            </div>
            <div className="field">
              <label htmlFor="recipe-wordpress-domain">Domain</label>
              <input
                className="input"
                id="recipe-wordpress-domain"
                onChange={(event) =>
                  setWordPressForm((current) => ({ ...current, domain: event.target.value }))
                }
                value={wordpressForm.domain}
              />
            </div>
            <div className="field">
              <label htmlFor="recipe-wordpress-php">PHP Version</label>
              <select
                className="select"
                id="recipe-wordpress-php"
                onChange={(event) =>
                  setWordPressForm((current) => ({ ...current, phpVersion: event.target.value }))
                }
                value={wordpressForm.phpVersion}
              >
                {phpVersionOptions.map((version) => (
                  <option key={version} value={version}>
                    PHP {version}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label htmlFor="recipe-wordpress-server">Web Server</label>
              <select
                className="select"
                id="recipe-wordpress-server"
                onChange={(event) =>
                  setWordPressForm((current) => ({
                    ...current,
                    serverType: event.target.value as ServerType,
                  }))
                }
                value={wordpressForm.serverType}
              >
                <option value="apache">Apache</option>
                <option value="nginx">Nginx</option>
                <option value="frankenphp">FrankenPHP (Experimental)</option>
              </select>
            </div>
          </div>
          <label className="checkbox-row">
            <input
              checked={wordpressForm.sslEnabled}
              onChange={(event) =>
                setWordPressForm((current) => ({ ...current, sslEnabled: event.target.checked }))
              }
              type="checkbox"
            />
            <span>Enable local HTTPS for this project profile</span>
          </label>
          </RecipeCard>
        </div>

        <div
          aria-labelledby="workspace-tab-git"
          className="workspace-panel"
          hidden={activeTab !== "git"}
          id="workspace-panel-git"
          role="tabpanel"
        >
          <RecipeCard
            action={
              <Button disabled={actionKey !== null} onClick={() => void handleGitClone()} variant="primary">
                {actionKey === "git" ? "Cloning..." : "Clone Repository"}
              </Button>
            }
            description="Clone a Git repository into a new folder, let DevNest scan it, then open the project detail right away."
            helper="Requires Git on the machine. Use this when the project already lives in a repo and you want the import plus clone to be one flow."
            title="Clone From Git"
          >
        <div className="form-grid">
          <div className="field">
            <label htmlFor="recipe-git-url">Repository URL</label>
            <input
              className="input"
              id="recipe-git-url"
              onChange={(event) =>
                setGitForm((current) => ({ ...current, repositoryUrl: event.target.value }))
              }
              placeholder="https://github.com/acme/shop-api.git"
              value={gitForm.repositoryUrl}
            />
          </div>
          <div className="field">
            <label htmlFor="recipe-git-branch">Branch</label>
            <input
              className="input"
              id="recipe-git-branch"
              onChange={(event) =>
                setGitForm((current) => ({ ...current, branch: event.target.value }))
              }
              placeholder="main"
              value={gitForm.branch}
            />
          </div>
          <div className="field">
            <label htmlFor="recipe-git-path">Target Path</label>
            <div className="recipe-path-field">
              <input
                className="input mono"
                id="recipe-git-path"
                onChange={(event) =>
                  setGitForm((current) => ({
                    ...current,
                    path: event.target.value,
                    domain: current.domain || inferDomainFromPath(event.target.value),
                  }))
                }
                placeholder="D:/Sites/shop-api"
                value={gitForm.path}
              />
              <Button
                disabled={actionKey !== null || pathPickerKey !== null}
                onClick={() => void handlePickTargetPath("git")}
                variant="ghost"
              >
                {pathPickerKey === "git" ? "Picking..." : "Browse"}
              </Button>
            </div>
            <span className="helper-text">Pick a parent folder. DevNest uses the repo or domain to suggest the project folder name automatically.</span>
          </div>
          <div className="field">
            <label htmlFor="recipe-git-domain">Domain</label>
            <input
              className="input"
              id="recipe-git-domain"
              onChange={(event) =>
                setGitForm((current) => ({ ...current, domain: event.target.value }))
              }
              value={gitForm.domain}
            />
          </div>
          <div className="field">
            <label htmlFor="recipe-git-php">PHP Version</label>
            <select
              className="select"
              id="recipe-git-php"
              onChange={(event) =>
                setGitForm((current) => ({ ...current, phpVersion: event.target.value }))
              }
              value={gitForm.phpVersion}
            >
              {phpVersionOptions.map((version) => (
                <option key={version} value={version}>
                  PHP {version}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="recipe-git-server">Web Server</label>
            <select
              className="select"
              id="recipe-git-server"
              onChange={(event) =>
                setGitForm((current) => ({
                  ...current,
                  serverType: event.target.value as ServerType,
                }))
              }
              value={gitForm.serverType}
            >
              <option value="apache">Apache</option>
              <option value="nginx">Nginx</option>
              <option value="frankenphp">FrankenPHP (Experimental)</option>
            </select>
          </div>
        </div>
        <label className="checkbox-row">
          <input
            checked={gitForm.sslEnabled}
            onChange={(event) =>
              setGitForm((current) => ({ ...current, sslEnabled: event.target.checked }))
            }
            type="checkbox"
          />
          <span>Enable local HTTPS for this project profile</span>
        </label>
          </RecipeCard>
        </div>
      </div>
    </div>
  );
}
