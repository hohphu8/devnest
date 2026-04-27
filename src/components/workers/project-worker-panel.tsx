import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useProjectStore } from "@/app/store/project-store";
import { useProjectWorkerStore } from "@/app/store/project-worker-store";
import { useToastStore } from "@/app/store/toast-store";
import { useWorkspaceStore } from "@/app/store/workspace-store";
import { ActionMenu, ActionMenuItem } from "@/components/ui/action-menu";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { projectApi } from "@/lib/api/project-api";
import { getAppErrorMessage } from "@/lib/tauri";
import {
  projectNameSchema,
  workerCommandLineSchema,
  workerWorkingDirectorySchema,
} from "@/lib/validators";
import { formatUpdatedAt } from "@/lib/utils";
import type {
  CreateProjectWorkerInput,
  ProjectWorker,
  ProjectWorkerPresetType,
  ProjectWorkerStatus,
  UpdateProjectWorkerPatch,
} from "@/types/project-worker";
import type { Project } from "@/types/project";

interface ProjectWorkerPanelProps {
  mode: "project" | "workspace";
  project?: Project;
}

interface WorkerFormState {
  projectId: string;
  name: string;
  presetType: ProjectWorkerPresetType;
  commandLine: string;
  workingDirectory: string;
  autoStart: boolean;
}

function defaultCommandLine(presetType: ProjectWorkerPresetType) {
  switch (presetType) {
    case "queue":
      return "php artisan queue:work";
    case "schedule":
      return "php artisan schedule:work";
    case "custom":
      return "";
  }
}

function defaultForm(project?: Project): WorkerFormState {
  return {
    projectId: project?.id ?? "",
    name: "",
    presetType: "queue",
    commandLine: defaultCommandLine("queue"),
    workingDirectory: project?.path ?? "",
    autoStart: true,
  };
}

function statusTone(status: ProjectWorkerStatus): "success" | "warning" | "error" {
  switch (status) {
    case "running":
      return "success";
    case "starting":
    case "restarting":
      return "warning";
    case "error":
      return "error";
    case "stopped":
      return "warning";
  }
}

function commandLineForWorker(worker: ProjectWorker) {
  return [worker.command, ...worker.args].filter(Boolean).join(" ");
}

function labelForPreset(presetType: ProjectWorkerPresetType) {
  switch (presetType) {
    case "queue":
      return "Laravel Queue";
    case "schedule":
      return "Laravel Schedule";
    case "custom":
      return "Custom Command";
  }
}

export function ProjectWorkerPanel({ mode, project }: ProjectWorkerPanelProps) {
  const navigate = useNavigate();
  const pushToast = useToastStore((state) => state.push);
  const workspaceLoaded = useWorkspaceStore((state) => state.loaded);
  const projects = useProjectStore((state) => state.projects);
  const {
    actionWorkerId,
    createWorker,
    deleteWorker,
    error,
    loadProjectWorkers,
    loadWorkers,
    restartWorker,
    startWorker,
    stopWorker,
    updateWorker,
    workers,
  } = useProjectWorkerStore();
  const [form, setForm] = useState<WorkerFormState>(() => defaultForm(project));
  const [editingWorkerId, setEditingWorkerId] = useState<string>();
  const [formMessage, setFormMessage] = useState<string>();
  const [loadingWorkers, setLoadingWorkers] = useState(false);

  useEffect(() => {
    setForm(defaultForm(project));
    setEditingWorkerId(undefined);
    setFormMessage(undefined);
  }, [project?.id, project?.path]);

  useEffect(() => {
    if (workspaceLoaded) {
      setLoadingWorkers(false);
      return;
    }

    let cancelled = false;
    setLoadingWorkers(true);

    const task =
      mode === "project" && project
        ? loadProjectWorkers(project.id)
        : loadWorkers();

    void task
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) {
          setLoadingWorkers(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [loadProjectWorkers, loadWorkers, mode, project, workspaceLoaded]);

  const visibleWorkers = useMemo(
    () =>
      mode === "project" && project
        ? workers.filter((worker) => worker.projectId === project.id)
        : workers,
    [mode, project, workers],
  );
  const projectMap = useMemo(
    () => Object.fromEntries(projects.map((item) => [item.id, item])),
    [projects],
  );
  const runningWorkers = visibleWorkers.filter((worker) => worker.status === "running").length;

  function resetForm(targetProject?: Project) {
    setForm(defaultForm(targetProject));
    setEditingWorkerId(undefined);
    setFormMessage(undefined);
  }

  function applyPreset(nextPreset: ProjectWorkerPresetType) {
    setForm((current) => ({
      ...current,
      presetType: nextPreset,
      commandLine:
        current.commandLine.trim().length === 0 ||
        current.commandLine === defaultCommandLine(current.presetType)
          ? defaultCommandLine(nextPreset)
          : current.commandLine,
    }));
  }

  function validateForm() {
    const projectId = (mode === "project" ? project?.id : form.projectId) ?? "";
    if (!projectId) {
      return "Select a project before saving this worker.";
    }

    const nameResult = projectNameSchema.safeParse(form.name);
    if (!nameResult.success) {
      return nameResult.error.issues[0]?.message ?? "Worker name is invalid.";
    }

    const commandLineResult = workerCommandLineSchema.safeParse(form.commandLine);
    if (!commandLineResult.success) {
      return commandLineResult.error.issues[0]?.message ?? "Worker command is invalid.";
    }

    const workingDirectoryResult = workerWorkingDirectorySchema.safeParse(form.workingDirectory);
    if (!workingDirectoryResult.success) {
      return (
        workingDirectoryResult.error.issues[0]?.message ?? "Working directory is invalid."
      );
    }

    return undefined;
  }

  async function handleChooseWorkingDirectory() {
    try {
      const selectedPath = await projectApi.pickFolder();
      if (!selectedPath) {
        return;
      }

      setForm((current) => ({ ...current, workingDirectory: selectedPath }));
      setFormMessage(undefined);
    } catch (invokeError) {
      const message = getAppErrorMessage(
        invokeError,
        "Could not open the folder picker for this worker.",
      );
      setFormMessage(message);
      pushToast({
        tone: "error",
        title: "Folder picker failed",
        message,
      });
    }
  }

  async function handleSubmit() {
    const validationMessage = validateForm();
    if (validationMessage) {
      setFormMessage(validationMessage);
      return;
    }

    try {
      const targetProjectId = mode === "project" ? project!.id : form.projectId;
      if (editingWorkerId) {
        const patch: UpdateProjectWorkerPatch = {
          name: form.name,
          presetType: form.presetType,
          commandLine: form.commandLine,
          workingDirectory: form.workingDirectory,
          autoStart: form.autoStart,
        };
        await updateWorker(editingWorkerId, patch);
        pushToast({
          tone: "success",
          title: "Worker updated",
          message: `${form.name} was updated successfully.`,
        });
      } else {
        const input: CreateProjectWorkerInput = {
          projectId: targetProjectId,
          name: form.name,
          presetType: form.presetType,
          commandLine: form.commandLine,
          workingDirectory: form.workingDirectory,
          autoStart: form.autoStart,
        };
        await createWorker(input);
        pushToast({
          tone: "success",
          title: "Worker created",
          message: `${form.name} is now tracked in DevNest.`,
        });
      }

      resetForm(mode === "project" ? project : projectMap[targetProjectId]);
    } catch (invokeError) {
      const message = getAppErrorMessage(invokeError, "Worker save failed.");
      setFormMessage(message);
      pushToast({
        tone: "error",
        title: editingWorkerId ? "Worker update failed" : "Worker create failed",
        message,
      });
    }
  }

  async function handleDelete(worker: ProjectWorker) {
    try {
      await deleteWorker(worker.id);
      if (editingWorkerId === worker.id) {
        resetForm(projectMap[worker.projectId]);
      }
      pushToast({
        tone: "success",
        title: "Worker deleted",
        message: `${worker.name} was removed from DevNest.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Worker delete failed",
        message: getAppErrorMessage(invokeError, "Could not delete the selected worker."),
      });
    }
  }

  async function handleWorkerAction(
    worker: ProjectWorker,
    action: "start" | "stop" | "restart",
  ) {
    try {
      if (action === "start") {
        await startWorker(worker.id);
      } else if (action === "stop") {
        await stopWorker(worker.id);
      } else {
        await restartWorker(worker.id);
      }

      pushToast({
        tone: action === "stop" ? "info" : "success",
        title: `${worker.name} ${action}ed`,
        message:
          action === "restart"
            ? `${worker.name} was restarted.`
            : `${worker.name} is now ${action === "start" ? "running" : "stopped"}.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: `${worker.name} ${action} failed`,
        message: getAppErrorMessage(invokeError, `Could not ${action} ${worker.name}.`),
      });
    }
  }

  function handleEdit(worker: ProjectWorker) {
    setEditingWorkerId(worker.id);
    setForm({
      projectId: worker.projectId,
      name: worker.name,
      presetType: worker.presetType,
      commandLine: commandLineForWorker(worker),
      workingDirectory: worker.workingDirectory,
      autoStart: worker.autoStart,
    });
    setFormMessage(undefined);
  }

  return (
    <div className="stack" style={{ gap: 16 }}>
      <Card>
        <div className="page-header">
          <div>
            <h2>{mode === "project" ? "Task/Cron Monitor" : "Workers"}</h2>
            <p>
              {mode === "project"
                ? "Manage background queue, schedule, and custom commands for this project without leaving a terminal window open."
                : "Monitor every managed project worker in one place, then jump into logs or project context as needed."}
            </p>
          </div>
          <div className="page-toolbar">
            <span className="status-chip" data-tone={runningWorkers > 0 ? "success" : "warning"}>
              {runningWorkers} running
            </span>
            {(editingWorkerId || form.name || form.commandLine || form.workingDirectory) &&
            (editingWorkerId || form.name.trim().length > 0) ? (
              <Button onClick={() => resetForm(project)}>Reset</Button>
            ) : null}
          </div>
        </div>

        <div className="form-grid">
          {mode === "workspace" ? (
            <div className="field">
              <label htmlFor="worker-project">Project</label>
              <select
                className="select"
                disabled={Boolean(editingWorkerId)}
                id="worker-project"
                onChange={(event) => {
                  const nextProject = projectMap[event.target.value];
                  setForm((current) => ({
                    ...current,
                    projectId: event.target.value,
                    workingDirectory: nextProject?.path ?? current.workingDirectory,
                  }));
                }}
                value={form.projectId}
              >
                <option value="">Select project</option>
                {projects.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </div>
          ) : null}

          <div className="field">
            <label htmlFor="worker-name">Display Name</label>
            <input
              className="input"
              id="worker-name"
              onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
              placeholder="Queue Worker"
              value={form.name}
            />
          </div>

          <div className="field">
            <label htmlFor="worker-preset">Preset</label>
            <select
              className="select"
              id="worker-preset"
              onChange={(event) => applyPreset(event.target.value as ProjectWorkerPresetType)}
              value={form.presetType}
            >
              <option value="queue">Laravel Queue</option>
              <option value="schedule">Laravel Schedule</option>
              <option value="custom">Custom Command</option>
            </select>
          </div>

          <div className="field">
            <label htmlFor="worker-command">Command Line</label>
            <input
              className="input mono"
              id="worker-command"
              onChange={(event) =>
                setForm((current) => ({ ...current, commandLine: event.target.value }))
              }
              placeholder="php artisan queue:work"
              value={form.commandLine}
            />
          </div>

          <div className="field" data-span="2">
            <label htmlFor="worker-directory">Working Directory</label>
            <div className="page-toolbar" style={{ alignItems: "stretch", justifyContent: "flex-start" }}>
              <input
                className="input mono"
                id="worker-directory"
                onChange={(event) =>
                  setForm((current) => ({ ...current, workingDirectory: event.target.value }))
                }
                placeholder={project?.path ?? "D:/Projects/example"}
                style={{ flex: 1 }}
                value={form.workingDirectory}
              />
              <Button onClick={() => void handleChooseWorkingDirectory()} type="button">
                Choose Folder
              </Button>
            </div>
          </div>
        </div>

        <label className="checkbox-row">
          <input
            checked={form.autoStart}
            onChange={(event) =>
              setForm((current) => ({ ...current, autoStart: event.target.checked }))
            }
            type="checkbox"
          />
          <span>Auto-start this worker the next time DevNest launches.</span>
        </label>

        {formMessage || error ? (
          <span className="error-text">{formMessage ?? error}</span>
        ) : (
          <span className="helper-text">
            Presets keep the command project-first. Custom workers still run as detached background
            processes with managed logs.
          </span>
        )}

        <div className="page-toolbar" style={{ justifyContent: "space-between" }}>
          <span className="helper-text mono">{form.commandLine || "No command selected yet."}</span>
          <div className="runtime-table-actions">
            {editingWorkerId ? (
              <Button onClick={() => resetForm(project)}>Cancel Edit</Button>
            ) : null}
            <Button
              busy={Boolean(actionWorkerId && editingWorkerId === actionWorkerId)}
              busyLabel={editingWorkerId ? "Saving worker..." : "Creating worker..."}
              onClick={() => void handleSubmit()}
              variant="primary"
            >
              {editingWorkerId ? "Save Worker" : "Add Worker"}
            </Button>
          </div>
        </div>
      </Card>

      <Card>
        <div className="page-header">
          <div>
            <h2>{mode === "project" ? "Managed Workers" : "All Managed Workers"}</h2>
            <p>
              Start, stop, restart, inspect logs, and review the last human-readable failure from
              the same desktop control surface.
            </p>
          </div>
          <Button onClick={() => void (mode === "project" && project ? loadProjectWorkers(project.id) : loadWorkers())}>
            Refresh
          </Button>
        </div>

        {loadingWorkers ? (
          <span className="helper-text">Loading worker state...</span>
        ) : visibleWorkers.length === 0 ? (
          <EmptyState
            actions={
              <Button onClick={() => setForm((current) => ({ ...current, name: "Queue Worker" }))}>
                Add First Worker
              </Button>
            }
            description={
              mode === "project"
                ? "Add a Laravel queue, schedule, or custom command worker for this project."
                : "No managed workers exist yet. Create one from a project detail view or here."
            }
            icon="server"
            title="No workers yet"
          />
        ) : (
          <div className="runtime-table-shell">
            <table className="runtime-table">
              <thead>
                <tr>
                  <th>Name</th>
                  {mode === "workspace" ? <th>Project</th> : null}
                  <th>Command</th>
                  <th>Status</th>
                  <th>PID</th>
                  <th>Auto</th>
                  <th>Updated</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {visibleWorkers.map((worker) => {
                  const relatedProject = projectMap[worker.projectId];
                  const busy = actionWorkerId === worker.id;
                  const isRunning = worker.status === "running";

                  return (
                    <tr key={worker.id}>
                      <td>
                        <div className="runtime-table-type">
                          <strong>{worker.name}</strong>
                          <span className="runtime-table-note">
                            {labelForPreset(worker.presetType)}
                          </span>
                          {worker.lastError ? (
                            <span className="error-text">{worker.lastError}</span>
                          ) : null}
                        </div>
                      </td>
                      {mode === "workspace" ? (
                        <td>
                          <div className="runtime-table-type">
                            <strong>{relatedProject?.name ?? "Unknown project"}</strong>
                            {relatedProject ? (
                              <span className="runtime-table-note">{relatedProject.domain}</span>
                            ) : null}
                          </div>
                        </td>
                      ) : null}
                      <td>
                        <div className="runtime-table-type">
                          <strong className="mono">{commandLineForWorker(worker)}</strong>
                          <span className="runtime-table-note mono">
                            {worker.workingDirectory}
                          </span>
                        </div>
                      </td>
                      <td>
                        <span className="status-chip" data-tone={statusTone(worker.status)}>
                          {worker.status}
                        </span>
                      </td>
                      <td>{worker.pid ?? "-"}</td>
                      <td>{worker.autoStart ? "On" : "Off"}</td>
                      <td>{formatUpdatedAt(worker.updatedAt)}</td>
                      <td>
                        {mode === "workspace" ? (
                          <div className="runtime-table-actions runtime-table-actions-compact">
                            <ActionMenu disabled={busy} label="Actions">
                              <ActionMenuItem
                                onClick={() =>
                                  void handleWorkerAction(worker, isRunning ? "stop" : "start")
                                }
                              >
                                {isRunning ? "Stop Worker" : "Start Worker"}
                              </ActionMenuItem>
                              <ActionMenuItem
                                onClick={() => void handleWorkerAction(worker, "restart")}
                              >
                                Restart Worker
                              </ActionMenuItem>
                              <ActionMenuItem
                                onClick={() =>
                                  navigate(`/logs?type=worker&workerId=${worker.id}`)
                                }
                              >
                                View Logs
                              </ActionMenuItem>
                              <ActionMenuItem onClick={() => handleEdit(worker)}>
                                Edit Worker
                              </ActionMenuItem>
                              {relatedProject ? (
                                <ActionMenuItem
                                  onClick={() =>
                                    navigate(`/projects?projectId=${relatedProject.id}`)
                                  }
                                >
                                  Open Project
                                </ActionMenuItem>
                              ) : null}
                              <ActionMenuItem
                                onClick={() => void handleDelete(worker)}
                                tone="danger"
                              >
                                Delete Worker
                              </ActionMenuItem>
                            </ActionMenu>
                          </div>
                        ) : (
                          <div className="runtime-table-actions">
                            <Button
                              busy={busy && !isRunning}
                              busyLabel={isRunning ? undefined : "Working..."}
                              onClick={() => void handleWorkerAction(worker, isRunning ? "stop" : "start")}
                              variant={isRunning ? "secondary" : "primary"}
                            >
                              {isRunning ? "Stop" : "Start"}
                            </Button>
                            <Button
                              busy={busy && worker.status === "restarting"}
                              busyLabel="Restarting..."
                              onClick={() => void handleWorkerAction(worker, "restart")}
                            >
                              Restart
                            </Button>
                            <Button
                              onClick={() =>
                                navigate(`/logs?type=worker&workerId=${worker.id}`)
                              }
                            >
                              View Logs
                            </Button>
                            <Button onClick={() => handleEdit(worker)}>Edit</Button>
                            <Button
                              className="button-danger"
                              onClick={() => void handleDelete(worker)}
                            >
                              Delete
                            </Button>
                          </div>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </div>
  );
}
