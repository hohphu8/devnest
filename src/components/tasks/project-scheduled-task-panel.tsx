import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useProjectScheduledTaskStore } from "@/app/store/project-scheduled-task-store";
import { useProjectStore } from "@/app/store/project-store";
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
  scheduledTaskCommandLineSchema,
  scheduledTaskCronSchema,
  scheduledTaskDailyTimeSchema,
  scheduledTaskIntervalSecondsSchema,
  scheduledTaskUrlSchema,
  scheduledTaskWorkingDirectorySchema,
} from "@/lib/validators";
import { formatUpdatedAt, formatUpdatedAtWithSeconds, serializeCommandLine } from "@/lib/utils";
import type {
  CreateProjectScheduledTaskInput,
  ProjectScheduledTask,
  ProjectScheduledTaskRun,
  ProjectScheduledTaskScheduleMode,
  ProjectScheduledTaskSimpleScheduleKind,
  ProjectScheduledTaskStatus,
  ProjectScheduledTaskType,
  UpdateProjectScheduledTaskPatch,
} from "@/types/project-scheduled-task";
import type { Project } from "@/types/project";

interface ProjectScheduledTaskPanelProps {
  mode: "project" | "workspace";
  project?: Project;
}

interface TaskFormState {
  projectId: string;
  name: string;
  taskType: ProjectScheduledTaskType;
  scheduleMode: ProjectScheduledTaskScheduleMode;
  simpleScheduleKind: ProjectScheduledTaskSimpleScheduleKind;
  intervalValue: string;
  dailyTime: string;
  weeklyDay: string;
  cronExpression: string;
  commandLine: string;
  url: string;
  workingDirectory: string;
  enabled: boolean;
  autoResume: boolean;
}

function defaultForm(project?: Project): TaskFormState {
  return {
    projectId: project?.id ?? "",
    name: "",
    taskType: "command",
    scheduleMode: "simple",
    simpleScheduleKind: "everyMinutes",
    intervalValue: "5",
    dailyTime: "08:00",
    weeklyDay: "0",
    cronExpression: "*/15 * * * *",
    commandLine: "php artisan schedule:run",
    url: "http://127.0.0.1:8000/health",
    workingDirectory: project?.path ?? "",
    enabled: true,
    autoResume: true,
  };
}

function statusTone(status: ProjectScheduledTaskStatus): "success" | "warning" | "error" {
  switch (status) {
    case "running":
    case "success":
      return "success";
    case "scheduled":
    case "idle":
    case "skipped":
      return "warning";
    case "error":
      return "error";
  }
}

function targetLabel(task: ProjectScheduledTask) {
  if (task.taskType === "url") {
    return task.url ?? "URL target missing";
  }

  return serializeCommandLine(task.command, task.args);
}

function runStatusTone(status: ProjectScheduledTaskRun["status"]): "success" | "warning" | "error" {
  switch (status) {
    case "success":
      return "success";
    case "running":
    case "skipped":
      return "warning";
    case "error":
      return "error";
  }
}

function formatSimpleInterval(task: ProjectScheduledTask): string {
  const value = task.intervalSeconds ?? 0;
  switch (task.simpleScheduleKind) {
    case "everySeconds":
      return value > 0 ? String(value) : "5";
    case "everyMinutes":
      return value > 0 ? String(Math.max(1, Math.round(value / 60))) : "5";
    case "everyHours":
      return value > 0 ? String(Math.max(1, Math.round(value / 3600))) : "1";
    case "daily":
    case "weekly":
    case undefined:
    case null:
      return "5";
  }
}

function applyTaskToForm(task: ProjectScheduledTask): TaskFormState {
  return {
    projectId: task.projectId,
    name: task.name,
    taskType: task.taskType,
    scheduleMode: task.scheduleMode,
    simpleScheduleKind: task.simpleScheduleKind ?? "everyMinutes",
    intervalValue: formatSimpleInterval(task),
    dailyTime: task.dailyTime ?? "08:00",
    weeklyDay: String(task.weeklyDay ?? 0),
    cronExpression: task.scheduleMode === "cron" ? task.scheduleExpression : "*/15 * * * *",
    commandLine: serializeCommandLine(task.command, task.args),
    url: task.url ?? "",
    workingDirectory: task.workingDirectory ?? "",
    enabled: task.enabled,
    autoResume: task.autoResume,
  };
}

function buildIntervalSeconds(form: TaskFormState): number {
  const parsed = Number(form.intervalValue);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return 0;
  }

  switch (form.simpleScheduleKind) {
    case "everySeconds":
      return parsed;
    case "everyMinutes":
      return parsed * 60;
    case "everyHours":
      return parsed * 3600;
    case "daily":
    case "weekly":
      return 0;
  }
}

function weekdayLabel(value: number): string {
  switch (value) {
    case 0:
      return "Monday";
    case 1:
      return "Tuesday";
    case 2:
      return "Wednesday";
    case 3:
      return "Thursday";
    case 4:
      return "Friday";
    case 5:
      return "Saturday";
    case 6:
      return "Sunday";
    default:
      return `Day ${value}`;
  }
}

export function ProjectScheduledTaskPanel({
  mode,
  project,
}: ProjectScheduledTaskPanelProps) {
  const navigate = useNavigate();
  const pushToast = useToastStore((state) => state.push);
  const workspaceLoaded = useWorkspaceStore((state) => state.loaded);
  const projects = useProjectStore((state) => state.projects);
  const {
    actionTaskId,
    clearTaskHistory,
    createTask,
    deleteTask,
    disableTask,
    enableTask,
    error,
    loadProjectTasks,
    loadTaskRuns,
    loadTasks,
    runTaskNow,
    runsByTaskId,
    tasks,
    updateTask,
  } = useProjectScheduledTaskStore();
  const [form, setForm] = useState<TaskFormState>(() => defaultForm(project));
  const [editingTaskId, setEditingTaskId] = useState<string>();
  const [formMessage, setFormMessage] = useState<string>();
  const [loadingTasks, setLoadingTasks] = useState(false);
  const [historyTaskId, setHistoryTaskId] = useState<string>();
  const [confirmDeleteTaskId, setConfirmDeleteTaskId] = useState<string>();

  useEffect(() => {
    setForm(defaultForm(project));
    setEditingTaskId(undefined);
    setHistoryTaskId(undefined);
    setConfirmDeleteTaskId(undefined);
    setFormMessage(undefined);
  }, [project?.id, project?.path]);

  useEffect(() => {
    if (workspaceLoaded) {
      setLoadingTasks(false);
      return;
    }

    let cancelled = false;
    setLoadingTasks(true);

    const task =
      mode === "project" && project ? loadProjectTasks(project.id) : loadTasks();

    void task
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) {
          setLoadingTasks(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [loadProjectTasks, loadTasks, mode, project, workspaceLoaded]);

  const visibleTasks = useMemo(
    () =>
      mode === "project" && project
        ? tasks.filter((task) => task.projectId === project.id)
        : tasks,
    [mode, project, tasks],
  );
  const projectMap = useMemo(
    () => Object.fromEntries(projects.map((item) => [item.id, item])),
    [projects],
  );
  const selectedHistoryTask =
    visibleTasks.find((task) => task.id === historyTaskId) ??
    (historyTaskId ? tasks.find((task) => task.id === historyTaskId) : undefined);
  const historyRuns = historyTaskId ? runsByTaskId[historyTaskId] ?? [] : [];
  const confirmDeleteTask =
    visibleTasks.find((task) => task.id === confirmDeleteTaskId) ??
    (confirmDeleteTaskId ? tasks.find((task) => task.id === confirmDeleteTaskId) : undefined);

  useEffect(() => {
    if (!selectedHistoryTask) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      event.preventDefault();
      setHistoryTaskId(undefined);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [selectedHistoryTask]);

  function resetForm(targetProject?: Project) {
    setForm(defaultForm(targetProject));
    setEditingTaskId(undefined);
    setFormMessage(undefined);
  }

  function validateForm() {
    const projectId = (mode === "project" ? project?.id : form.projectId) ?? "";
    if (!projectId) {
      return "Select a project before saving this scheduled task.";
    }

    const nameResult = projectNameSchema.safeParse(form.name);
    if (!nameResult.success) {
      return nameResult.error.issues[0]?.message ?? "Task name is invalid.";
    }

    if (form.taskType === "command") {
      const commandLineResult = scheduledTaskCommandLineSchema.safeParse(form.commandLine);
      if (!commandLineResult.success) {
        return commandLineResult.error.issues[0]?.message ?? "Task command is invalid.";
      }

      const workingDirectoryResult = scheduledTaskWorkingDirectorySchema.safeParse(
        form.workingDirectory,
      );
      if (!workingDirectoryResult.success) {
        return (
          workingDirectoryResult.error.issues[0]?.message ?? "Working directory is invalid."
        );
      }
    } else {
      const urlResult = scheduledTaskUrlSchema.safeParse(form.url);
      if (!urlResult.success) {
        return urlResult.error.issues[0]?.message ?? "Task URL is invalid.";
      }
    }

    if (form.scheduleMode === "cron") {
      const cronResult = scheduledTaskCronSchema.safeParse(form.cronExpression);
      if (!cronResult.success) {
        return cronResult.error.issues[0]?.message ?? "Cron expression is invalid.";
      }
      return undefined;
    }

    if (
      form.simpleScheduleKind === "everySeconds" ||
      form.simpleScheduleKind === "everyMinutes" ||
      form.simpleScheduleKind === "everyHours"
    ) {
      const intervalResult = scheduledTaskIntervalSecondsSchema.safeParse(
        buildIntervalSeconds(form),
      );
      if (!intervalResult.success) {
        return intervalResult.error.issues[0]?.message ?? "Interval is invalid.";
      }
    }

    if (form.simpleScheduleKind === "daily" || form.simpleScheduleKind === "weekly") {
      const dailyTimeResult = scheduledTaskDailyTimeSchema.safeParse(form.dailyTime);
      if (!dailyTimeResult.success) {
        return dailyTimeResult.error.issues[0]?.message ?? "Daily time is invalid.";
      }
    }

    if (form.simpleScheduleKind === "weekly") {
      const weeklyDay = Number(form.weeklyDay);
      if (!Number.isInteger(weeklyDay) || weeklyDay < 0 || weeklyDay > 6) {
        return "Weekly schedule day must stay between Monday and Sunday.";
      }
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
        "Could not open the folder picker for this scheduled task.",
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
      const intervalSeconds = buildIntervalSeconds(form);
      if (editingTaskId) {
        const patch: UpdateProjectScheduledTaskPatch = {
          name: form.name,
          taskType: form.taskType,
          scheduleMode: form.scheduleMode,
          simpleScheduleKind: form.scheduleMode === "simple" ? form.simpleScheduleKind : null,
          scheduleExpression:
            form.scheduleMode === "cron" ? form.cronExpression : null,
          intervalSeconds:
            form.scheduleMode === "simple" &&
            (form.simpleScheduleKind === "everySeconds" ||
              form.simpleScheduleKind === "everyMinutes" ||
              form.simpleScheduleKind === "everyHours")
              ? intervalSeconds
              : null,
          dailyTime:
            form.scheduleMode === "simple" &&
            (form.simpleScheduleKind === "daily" || form.simpleScheduleKind === "weekly")
              ? form.dailyTime
              : null,
          weeklyDay:
            form.scheduleMode === "simple" && form.simpleScheduleKind === "weekly"
              ? Number(form.weeklyDay)
              : null,
          url: form.taskType === "url" ? form.url : null,
          commandLine: form.taskType === "command" ? form.commandLine : null,
          workingDirectory: form.taskType === "command" ? form.workingDirectory : null,
          enabled: form.enabled,
          autoResume: form.autoResume,
        };
        await updateTask(editingTaskId, patch);
        pushToast({
          tone: "success",
          title: "Scheduled task updated",
          message: `${form.name} was updated successfully.`,
        });
      } else {
        const input: CreateProjectScheduledTaskInput = {
          projectId: targetProjectId,
          name: form.name,
          taskType: form.taskType,
          scheduleMode: form.scheduleMode,
          simpleScheduleKind: form.scheduleMode === "simple" ? form.simpleScheduleKind : null,
          scheduleExpression: form.scheduleMode === "cron" ? form.cronExpression : null,
          intervalSeconds:
            form.scheduleMode === "simple" &&
            (form.simpleScheduleKind === "everySeconds" ||
              form.simpleScheduleKind === "everyMinutes" ||
              form.simpleScheduleKind === "everyHours")
              ? intervalSeconds
              : null,
          dailyTime:
            form.scheduleMode === "simple" &&
            (form.simpleScheduleKind === "daily" || form.simpleScheduleKind === "weekly")
              ? form.dailyTime
              : null,
          weeklyDay:
            form.scheduleMode === "simple" && form.simpleScheduleKind === "weekly"
              ? Number(form.weeklyDay)
              : null,
          url: form.taskType === "url" ? form.url : null,
          commandLine: form.taskType === "command" ? form.commandLine : null,
          workingDirectory: form.taskType === "command" ? form.workingDirectory : null,
          enabled: form.enabled,
          autoResume: form.autoResume,
        };
        await createTask(input);
        pushToast({
          tone: "success",
          title: "Scheduled task created",
          message: `${form.name} is now tracked in DevNest.`,
        });
      }

      resetForm(mode === "project" ? project : projectMap[targetProjectId]);
    } catch (invokeError) {
      const message = getAppErrorMessage(invokeError, "Scheduled task save failed.");
      setFormMessage(message);
      pushToast({
        tone: "error",
        title: editingTaskId ? "Task update failed" : "Task create failed",
        message,
      });
    }
  }

  async function handleDelete(task: ProjectScheduledTask) {
    try {
      await deleteTask(task.id);
      if (editingTaskId === task.id) {
        resetForm(projectMap[task.projectId]);
      }
      if (historyTaskId === task.id) {
        setHistoryTaskId(undefined);
      }
      setConfirmDeleteTaskId(undefined);
      pushToast({
        tone: "success",
        title: "Scheduled task deleted",
        message: `${task.name} was removed from DevNest.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Task delete failed",
        message: getAppErrorMessage(invokeError, "Could not delete the selected task."),
      });
    }
  }

  async function handleToggleEnabled(task: ProjectScheduledTask) {
    try {
      if (task.enabled) {
        await disableTask(task.id);
      } else {
        await enableTask(task.id);
      }

      pushToast({
        tone: task.enabled ? "info" : "success",
        title: task.enabled ? "Task disabled" : "Task enabled",
        message: `${task.name} is now ${task.enabled ? "disabled" : "scheduled"}.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: `${task.name} toggle failed`,
        message: getAppErrorMessage(
          invokeError,
          `Could not ${task.enabled ? "disable" : "enable"} ${task.name}.`,
        ),
      });
    }
  }

  async function handleRunNow(task: ProjectScheduledTask) {
    try {
      await runTaskNow(task.id);
      await loadTaskRuns(task.id, 10);
      setHistoryTaskId(task.id);
      pushToast({
        tone: "success",
        title: "Task dispatched",
        message: `${task.name} was triggered immediately.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: `${task.name} run failed`,
        message: getAppErrorMessage(invokeError, `Could not run ${task.name} now.`),
      });
    }
  }

  async function handleViewHistory(task: ProjectScheduledTask) {
    setHistoryTaskId(task.id);
    try {
      await loadTaskRuns(task.id, 25);
    } catch {
      // Store error banner is enough here.
    }
  }

  async function handleViewLatestLogs(task: ProjectScheduledTask) {
    try {
      const runs = await loadTaskRuns(task.id, 1);
      const latestRun = runs[0];
      if (!latestRun) {
        pushToast({
          tone: "warning",
          title: "No run logs yet",
          message: `${task.name} has not produced a run log yet.`,
        });
        return;
      }

      navigate(`/logs?type=scheduled-task-run&taskId=${task.id}&runId=${latestRun.id}`);
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Task logs unavailable",
        message: getAppErrorMessage(invokeError, "Could not load the latest task run logs."),
      });
    }
  }

  function handleEdit(task: ProjectScheduledTask) {
    setEditingTaskId(task.id);
    setForm(applyTaskToForm(task));
    setFormMessage(undefined);
  }

  function handleRequestDelete(task: ProjectScheduledTask) {
    setConfirmDeleteTaskId(task.id);
  }

  async function handleClearHistory(task: ProjectScheduledTask) {
    try {
      await clearTaskHistory(task.id);
      pushToast({
        tone: "success",
        title: "Task history cleared",
        message: `${task.name} history and logs were cleared.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Task history clear failed",
        message: getAppErrorMessage(invokeError, "Could not clear the selected task history."),
      });
    }
  }

  return (
    <div className="stack" style={{ gap: 16 }}>
      <Card>
        <div className="page-header">
          <div>
            <h2>{mode === "project" ? "Scheduled Tasks" : "All Scheduled Tasks"}</h2>
            <p>
              {mode === "project"
                ? "Schedule one-shot commands or URL hits for this project with simple intervals, cron, run history, and managed logs."
                : "Monitor and edit every managed scheduled task across the workspace from one control surface."}
            </p>
          </div>
          <div className="page-toolbar">
            <span
              className="status-chip"
              data-tone={visibleTasks.some((task) => task.enabled) ? "success" : "warning"}
            >
              {visibleTasks.filter((task) => task.enabled).length} enabled
            </span>
            {(editingTaskId || form.name.trim() || form.commandLine.trim() || form.url.trim()) ? (
              <Button onClick={() => resetForm(project)}>Reset</Button>
            ) : null}
          </div>
        </div>

        <div className="form-grid">
          {mode === "workspace" ? (
            <div className="field">
              <label htmlFor="task-project">Project</label>
              <select
                className="select"
                disabled={Boolean(editingTaskId)}
                id="task-project"
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
            <label htmlFor="task-name">Display Name</label>
            <input
              className="input"
              id="task-name"
              onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
              placeholder="Daily schedule run"
              value={form.name}
            />
          </div>

          <div className="field">
            <label htmlFor="task-type">Task Type</label>
            <select
              className="select"
              id="task-type"
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  taskType: event.target.value as ProjectScheduledTaskType,
                }))
              }
              value={form.taskType}
            >
              <option value="command">Command</option>
              <option value="url">URL (GET)</option>
            </select>
          </div>

          <div className="field">
            <label htmlFor="task-schedule-mode">Schedule Mode</label>
            <select
              className="select"
              id="task-schedule-mode"
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  scheduleMode: event.target.value as ProjectScheduledTaskScheduleMode,
                }))
              }
              value={form.scheduleMode}
            >
              <option value="simple">Simple</option>
              <option value="cron">Cron</option>
            </select>
          </div>

          {form.scheduleMode === "simple" ? (
            <>
              <div className="field">
                <label htmlFor="task-simple-kind">Schedule</label>
                <select
                  className="select"
                  id="task-simple-kind"
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      simpleScheduleKind: event.target.value as ProjectScheduledTaskSimpleScheduleKind,
                    }))
                  }
                  value={form.simpleScheduleKind}
                >
                  <option value="everySeconds">Every X seconds</option>
                  <option value="everyMinutes">Every X minutes</option>
                  <option value="everyHours">Every X hours</option>
                  <option value="daily">Daily at</option>
                  <option value="weekly">Weekly</option>
                </select>
              </div>

              {(form.simpleScheduleKind === "everySeconds" ||
                form.simpleScheduleKind === "everyMinutes" ||
                form.simpleScheduleKind === "everyHours") ? (
                <div className="field">
                  <label htmlFor="task-interval">Interval</label>
                  <input
                    className="input"
                    id="task-interval"
                    min={1}
                    onChange={(event) =>
                      setForm((current) => ({ ...current, intervalValue: event.target.value }))
                    }
                    type="number"
                    value={form.intervalValue}
                  />
                </div>
              ) : null}

              {(form.simpleScheduleKind === "daily" || form.simpleScheduleKind === "weekly") ? (
                <div className="field">
                  <label htmlFor="task-daily-time">Time</label>
                  <input
                    className="input mono"
                    id="task-daily-time"
                    onChange={(event) =>
                      setForm((current) => ({ ...current, dailyTime: event.target.value }))
                    }
                    placeholder="08:00"
                    value={form.dailyTime}
                  />
                </div>
              ) : null}

              {form.simpleScheduleKind === "weekly" ? (
                <div className="field">
                  <label htmlFor="task-weekly-day">Day</label>
                  <select
                    className="select"
                    id="task-weekly-day"
                    onChange={(event) =>
                      setForm((current) => ({ ...current, weeklyDay: event.target.value }))
                    }
                    value={form.weeklyDay}
                  >
                    {[0, 1, 2, 3, 4, 5, 6].map((day) => (
                      <option key={day} value={String(day)}>
                        {weekdayLabel(day)}
                      </option>
                    ))}
                  </select>
                </div>
              ) : null}
            </>
          ) : (
            <div className="field" data-span="2">
              <label htmlFor="task-cron">Cron Expression</label>
              <input
                className="input mono"
                id="task-cron"
                onChange={(event) =>
                  setForm((current) => ({ ...current, cronExpression: event.target.value }))
                }
                placeholder="*/15 * * * *"
                value={form.cronExpression}
              />
            </div>
          )}

          {form.taskType === "command" ? (
            <>
              <div className="field" data-span="2">
                <label htmlFor="task-command">Command Line</label>
                <input
                  className="input mono"
                  id="task-command"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, commandLine: event.target.value }))
                  }
                  placeholder="php artisan schedule:run"
                  value={form.commandLine}
                />
              </div>

              <div className="field" data-span="2">
                <label htmlFor="task-directory">Working Directory</label>
                <div
                  className="page-toolbar"
                  style={{ alignItems: "stretch", justifyContent: "flex-start" }}
                >
                  <input
                    className="input mono"
                    id="task-directory"
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        workingDirectory: event.target.value,
                      }))
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
            </>
          ) : (
            <div className="field" data-span="2">
              <label htmlFor="task-url">URL Target</label>
              <input
                className="input mono"
                id="task-url"
                onChange={(event) => setForm((current) => ({ ...current, url: event.target.value }))}
                placeholder="http://127.0.0.1:8000/health"
                value={form.url}
              />
            </div>
          )}
        </div>

        <label className="checkbox-row">
          <input
            checked={form.enabled}
            onChange={(event) => setForm((current) => ({ ...current, enabled: event.target.checked }))}
            type="checkbox"
          />
          <span>Enable this task immediately after saving.</span>
        </label>

        <label className="checkbox-row">
          <input
            checked={form.autoResume}
            onChange={(event) =>
              setForm((current) => ({ ...current, autoResume: event.target.checked }))
            }
            type="checkbox"
          />
          <span>Auto-resume this schedule the next time DevNest launches.</span>
        </label>

        {formMessage || error ? (
          <span className="error-text">{formMessage ?? error}</span>
        ) : (
          <span className="helper-text">
            Simple schedules are local-machine time based. Weekly day mapping uses Monday through
            Sunday. `Every X seconds` keeps a 5-second minimum.
          </span>
        )}

        <div className="page-toolbar" style={{ justifyContent: "space-between" }}>
          <span className="helper-text mono">
            {form.scheduleMode === "cron"
              ? form.cronExpression
              : form.taskType === "command"
                ? form.commandLine
                : form.url}
          </span>
          <div className="runtime-table-actions">
            {editingTaskId ? <Button onClick={() => resetForm(project)}>Cancel Edit</Button> : null}
            <Button
              busy={Boolean(actionTaskId && editingTaskId === actionTaskId)}
              busyLabel={editingTaskId ? "Saving task..." : "Creating task..."}
              onClick={() => void handleSubmit()}
              variant="primary"
            >
              {editingTaskId ? "Save Task" : "Add Task"}
            </Button>
          </div>
        </div>
      </Card>

      <Card>
        <div className="page-header">
          <div>
            <h2>{mode === "project" ? "Managed Schedule" : "Workspace Schedule"}</h2>
            <p>
              Enable, disable, run now, inspect history, and open logs from the same project-aware
              surface.
            </p>
          </div>
          <Button onClick={() => void (mode === "project" && project ? loadProjectTasks(project.id) : loadTasks())}>
            Refresh
          </Button>
        </div>

        {loadingTasks ? (
          <span className="helper-text">Loading scheduled task state...</span>
        ) : visibleTasks.length === 0 ? (
          <EmptyState
            actions={
              <Button onClick={() => setForm((current) => ({ ...current, name: "Schedule Run" }))}>
                Add First Task
              </Button>
            }
            description={
              mode === "project"
                ? "Add a recurring command or URL task for this project."
                : "No scheduled tasks exist yet. Create one here or from a project detail view."
            }
            icon="activity"
            title="No scheduled tasks yet"
          />
        ) : (
          <div className="runtime-table-shell">
            <table className="runtime-table">
              <thead>
                <tr>
                  <th>Name</th>
                  {mode === "workspace" ? <th>Project</th> : null}
                  <th>Target</th>
                  <th>Schedule</th>
                  <th>Status</th>
                  <th>Next Run</th>
                  <th>Updated</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {visibleTasks.map((task) => {
                  const relatedProject = projectMap[task.projectId];
                  const busy = actionTaskId === task.id;

                  return (
                    <tr key={task.id}>
                      <td>
                        <div className="runtime-table-type">
                          <strong>{task.name}</strong>
                          <span className="runtime-table-note">
                            {task.taskType === "command" ? "Command Task" : "URL Task"}
                          </span>
                          {task.lastError ? <span className="error-text">{task.lastError}</span> : null}
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
                          <strong className="mono">{targetLabel(task)}</strong>
                          {task.taskType === "command" && task.workingDirectory ? (
                            <span className="runtime-table-note mono">{task.workingDirectory}</span>
                          ) : null}
                        </div>
                      </td>
                      <td>
                        <div className="runtime-table-type">
                          <strong>{task.scheduleExpression}</strong>
                          <span className="runtime-table-note">
                            {task.autoResume ? "Auto-resume on" : "Auto-resume off"}
                          </span>
                        </div>
                      </td>
                      <td>
                        <span className="status-chip" data-tone={statusTone(task.status)}>
                          {task.enabled ? task.status : "disabled"}
                        </span>
                      </td>
                      <td>{task.nextRunAt ? formatUpdatedAt(task.nextRunAt) : "-"}</td>
                      <td>{formatUpdatedAt(task.updatedAt)}</td>
                      <td>
                        {mode === "workspace" ? (
                          <div className="runtime-table-actions runtime-table-actions-compact">
                            <ActionMenu disabled={busy} label="Actions">
                              <ActionMenuItem onClick={() => void handleToggleEnabled(task)}>
                                {task.enabled ? "Disable Task" : "Enable Task"}
                              </ActionMenuItem>
                              <ActionMenuItem onClick={() => void handleRunNow(task)}>
                                Run Now
                              </ActionMenuItem>
                              <ActionMenuItem onClick={() => void handleViewHistory(task)}>
                                View History
                              </ActionMenuItem>
                              <ActionMenuItem onClick={() => void handleViewLatestLogs(task)}>
                                View Logs
                              </ActionMenuItem>
                              <ActionMenuItem onClick={() => handleEdit(task)}>
                                Edit Task
                              </ActionMenuItem>
                              <ActionMenuItem onClick={() => void handleClearHistory(task)}>
                                Clear History
                              </ActionMenuItem>
                              {relatedProject ? (
                                <ActionMenuItem
                                  onClick={() => navigate(`/projects?projectId=${relatedProject.id}`)}
                                >
                                  Open Project
                                </ActionMenuItem>
                              ) : null}
                              <ActionMenuItem
                                onClick={() => handleRequestDelete(task)}
                                tone="danger"
                              >
                                Delete Task
                              </ActionMenuItem>
                            </ActionMenu>
                          </div>
                        ) : (
                          <div className="runtime-table-actions">
                            <Button
                              busy={busy}
                              busyLabel={task.enabled ? "Disabling..." : "Enabling..."}
                              onClick={() => void handleToggleEnabled(task)}
                              variant={task.enabled ? "secondary" : "primary"}
                            >
                              {task.enabled ? "Disable" : "Enable"}
                            </Button>
                            <Button onClick={() => void handleRunNow(task)}>Run Now</Button>
                            <Button onClick={() => void handleViewHistory(task)}>History</Button>
                            <Button onClick={() => void handleViewLatestLogs(task)}>Logs</Button>
                            <Button onClick={() => handleEdit(task)}>Edit</Button>
                            <Button
                              className="button-danger"
                              onClick={() => handleRequestDelete(task)}
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

      {selectedHistoryTask ? (
        <div
          className="wizard-overlay"
          onClick={() => setHistoryTaskId(undefined)}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="runtime-config-dialog scheduled-task-history-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="runtime-tools-header">
              <div>
                <h2>{selectedHistoryTask.name} History</h2>
                <p>Recent runs, durations, exit outcomes, and per-run logs for this scheduled task.</p>
              </div>
              <div className="runtime-table-actions">
                <Button
                  busy={actionTaskId === selectedHistoryTask.id}
                  busyLabel="Running task..."
                  onClick={() => void handleRunNow(selectedHistoryTask)}
                >
                  Run Now
                </Button>
                <Button
                  busy={actionTaskId === selectedHistoryTask.id}
                  busyLabel="Clearing history..."
                  onClick={() => void handleClearHistory(selectedHistoryTask)}
                >
                  Clear History
                </Button>
                <Button onClick={() => void handleViewHistory(selectedHistoryTask)}>
                  Refresh History
                </Button>
                <Button onClick={() => setHistoryTaskId(undefined)}>Close</Button>
              </div>
            </div>

            {historyRuns.length === 0 ? (
              <Card>
                <span className="helper-text">
                  No run history is recorded yet for this task.
                </span>
              </Card>
            ) : (
              <div className="runtime-table-shell">
                <table className="runtime-table">
                  <thead>
                    <tr>
                      <th>Started</th>
                      <th>Status</th>
                      <th>Duration</th>
                      <th>Result</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {historyRuns.map((run) => (
                      <tr key={run.id}>
                        <td>
                          <div className="runtime-table-type">
                            <strong>{formatUpdatedAtWithSeconds(run.startedAt)}</strong>
                            <span className="runtime-table-note mono">{run.id}</span>
                          </div>
                        </td>
                        <td>
                          <span className="status-chip" data-tone={runStatusTone(run.status)}>
                            {run.status}
                          </span>
                        </td>
                        <td>{run.durationMs != null ? `${run.durationMs} ms` : "-"}</td>
                        <td>
                          <div className="runtime-table-type">
                            <strong>
                              {run.responseStatus != null
                                ? `HTTP ${run.responseStatus}`
                                : run.exitCode != null
                                  ? `Exit ${run.exitCode}`
                                  : run.errorMessage
                                    ? "Error"
                                    : "Completed"}
                            </strong>
                            {run.errorMessage ? (
                              <span className="error-text">{run.errorMessage}</span>
                            ) : null}
                          </div>
                        </td>
                        <td>
                          <div className="runtime-table-actions">
                            <Button
                              onClick={() =>
                                navigate(
                                  `/logs?type=scheduled-task-run&taskId=${run.taskId}&runId=${run.id}`,
                                )
                              }
                            >
                              View Logs
                            </Button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      ) : null}

      {confirmDeleteTask ? (
        <div
          className="wizard-overlay"
          onClick={() => setConfirmDeleteTaskId(undefined)}
          role="dialog"
          aria-modal="true"
        >
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Delete scheduled task?</h3>
              <p>
                This will remove <strong>{confirmDeleteTask.name}</strong>, delete its managed log
                files, and remove its run history from DevNest.
              </p>
              <div className="detail-item">
                <span className="detail-label">Schedule</span>
                <strong>{confirmDeleteTask.scheduleExpression}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Target</span>
                <strong className="mono detail-value">{targetLabel(confirmDeleteTask)}</strong>
              </div>
              <span className="error-text">
                This action cannot be undone. Project files on disk are not deleted.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={actionTaskId === confirmDeleteTask.id}
                onClick={() => setConfirmDeleteTaskId(undefined)}
              >
                Cancel
              </Button>
              <Button
                busy={actionTaskId === confirmDeleteTask.id}
                busyLabel="Deleting task..."
                className="button-danger"
                onClick={() => void handleDelete(confirmDeleteTask)}
              >
                Delete Task
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
