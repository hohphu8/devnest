import { create } from "zustand";
import { runAsyncAction } from "@/app/store/async-action-store";
import { projectScheduledTaskApi } from "@/lib/api/project-scheduled-task-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type {
  CreateProjectScheduledTaskInput,
  ProjectScheduledTask,
  ProjectScheduledTaskRun,
  UpdateProjectScheduledTaskPatch,
} from "@/types/project-scheduled-task";

interface ProjectScheduledTaskStore {
  tasks: ProjectScheduledTask[];
  runsByTaskId: Record<string, ProjectScheduledTaskRun[]>;
  loaded: boolean;
  loading: boolean;
  actionTaskId?: string;
  error?: string;
  hydrateTasks: (tasks: ProjectScheduledTask[]) => void;
  loadTasks: () => Promise<void>;
  loadProjectTasks: (projectId: string) => Promise<ProjectScheduledTask[]>;
  fetchTaskStatus: (taskId: string) => Promise<ProjectScheduledTask>;
  createTask: (input: CreateProjectScheduledTaskInput) => Promise<ProjectScheduledTask>;
  updateTask: (taskId: string, patch: UpdateProjectScheduledTaskPatch) => Promise<ProjectScheduledTask>;
  deleteTask: (taskId: string) => Promise<void>;
  enableTask: (taskId: string) => Promise<ProjectScheduledTask>;
  disableTask: (taskId: string) => Promise<ProjectScheduledTask>;
  runTaskNow: (taskId: string) => Promise<ProjectScheduledTask>;
  loadTaskRuns: (taskId: string, limit?: number) => Promise<ProjectScheduledTaskRun[]>;
  clearTaskHistory: (taskId: string) => Promise<ProjectScheduledTask>;
  removeTasksForProject: (projectId: string) => void;
}

function upsertTask(
  tasks: ProjectScheduledTask[],
  nextTask: ProjectScheduledTask,
): ProjectScheduledTask[] {
  const exists = tasks.some((task) => task.id === nextTask.id);
  return exists
    ? tasks.map((task) => (task.id === nextTask.id ? nextTask : task))
    : [nextTask, ...tasks];
}

let loadTasksPromise: Promise<void> | undefined;

export const useProjectScheduledTaskStore = create<ProjectScheduledTaskStore>((set, get) => ({
  tasks: [],
  runsByTaskId: {},
  loaded: false,
  loading: false,
  actionTaskId: undefined,
  error: undefined,
  hydrateTasks: (tasks) =>
    set({
      tasks,
      loaded: true,
      loading: false,
      actionTaskId: undefined,
      error: undefined,
    }),
  loadTasks: async () => {
    if (loadTasksPromise) {
      return loadTasksPromise;
    }

    set({ loading: true, error: undefined });
    loadTasksPromise = (async () => {
      try {
        get().hydrateTasks(await projectScheduledTaskApi.listAll());
      } catch (error) {
        set({
          loaded: false,
          loading: false,
          actionTaskId: undefined,
          error: getAppErrorMessage(error, "Failed to load scheduled tasks."),
        });
      } finally {
        loadTasksPromise = undefined;
      }
    })();

    return loadTasksPromise;
  },
  loadProjectTasks: async (projectId) => {
    set({ loading: true, error: undefined });
    try {
      const projectTasks = await projectScheduledTaskApi.listByProject(projectId);
      set((state) => ({
        tasks: [...projectTasks, ...state.tasks.filter((task) => task.projectId !== projectId)],
        loaded: true,
        loading: false,
        actionTaskId: undefined,
        error: undefined,
      }));
      return projectTasks;
    } catch (error) {
      set({
        loading: false,
        actionTaskId: undefined,
        error: getAppErrorMessage(error, "Failed to load project scheduled tasks."),
      });
      throw error;
    }
  },
  fetchTaskStatus: async (taskId) => {
    set({ loading: true, error: undefined });
    try {
      const task = await projectScheduledTaskApi.getStatus(taskId);
      set((state) => ({
        tasks: upsertTask(state.tasks, task),
        loaded: true,
        loading: false,
        actionTaskId: undefined,
        error: undefined,
      }));
      return task;
    } catch (error) {
      set({
        loading: false,
        actionTaskId: undefined,
        error: getAppErrorMessage(error, "Failed to load scheduled task status."),
      });
      throw error;
    }
  },
  createTask: async (input) =>
    runAsyncAction(
      `scheduled-task:create:${input.projectId}:${input.name.trim().toLowerCase()}`,
      async () => {
        set({ loading: true, error: undefined });
        try {
          const task = await projectScheduledTaskApi.create(input);
          set((state) => ({
            tasks: upsertTask(state.tasks, task),
            loaded: true,
            loading: false,
            error: undefined,
          }));
          return task;
        } catch (error) {
          set({
            loading: false,
            error: getAppErrorMessage(error, "Failed to create scheduled task."),
          });
          throw error;
        }
      },
      "Creating task...",
    ),
  updateTask: async (taskId, patch) =>
    runAsyncAction(
      `scheduled-task:${taskId}:save`,
      async () => {
        set({ actionTaskId: taskId, error: undefined });
        try {
          const task = await projectScheduledTaskApi.update(taskId, patch);
          set((state) => ({
            tasks: upsertTask(state.tasks, task),
            actionTaskId: undefined,
            error: undefined,
          }));
          return task;
        } catch (error) {
          set({
            actionTaskId: undefined,
            error: getAppErrorMessage(error, "Failed to update scheduled task."),
          });
          throw error;
        }
      },
      "Saving task...",
    ),
  deleteTask: async (taskId) =>
    runAsyncAction(
      `scheduled-task:${taskId}:delete`,
      async () => {
        set({ actionTaskId: taskId, error: undefined });
        try {
          await projectScheduledTaskApi.remove(taskId);
          set((state) => {
            const nextRuns = { ...state.runsByTaskId };
            delete nextRuns[taskId];
            return {
              tasks: state.tasks.filter((task) => task.id !== taskId),
              runsByTaskId: nextRuns,
              actionTaskId: undefined,
              error: undefined,
            };
          });
        } catch (error) {
          set({
            actionTaskId: undefined,
            error: getAppErrorMessage(error, "Failed to delete scheduled task."),
          });
          throw error;
        }
      },
      "Deleting task...",
    ),
  enableTask: async (taskId) =>
    runAsyncAction(
      `scheduled-task:${taskId}:enable`,
      async () => {
        set({ actionTaskId: taskId, error: undefined });
        try {
          const task = await projectScheduledTaskApi.enable(taskId);
          set((state) => ({
            tasks: upsertTask(state.tasks, task),
            actionTaskId: undefined,
            error: undefined,
          }));
          return task;
        } catch (error) {
          set({
            actionTaskId: undefined,
            error: getAppErrorMessage(error, "Failed to enable scheduled task."),
          });
          throw error;
        }
      },
      "Enabling task...",
    ),
  disableTask: async (taskId) =>
    runAsyncAction(
      `scheduled-task:${taskId}:disable`,
      async () => {
        set({ actionTaskId: taskId, error: undefined });
        try {
          const task = await projectScheduledTaskApi.disable(taskId);
          set((state) => ({
            tasks: upsertTask(state.tasks, task),
            actionTaskId: undefined,
            error: undefined,
          }));
          return task;
        } catch (error) {
          set({
            actionTaskId: undefined,
            error: getAppErrorMessage(error, "Failed to disable scheduled task."),
          });
          throw error;
        }
      },
      "Disabling task...",
    ),
  runTaskNow: async (taskId) =>
    runAsyncAction(
      `scheduled-task:${taskId}:run-now`,
      async () => {
        set({ actionTaskId: taskId, error: undefined });
        try {
          const task = await projectScheduledTaskApi.runNow(taskId);
          set((state) => ({
            tasks: upsertTask(state.tasks, task),
            actionTaskId: undefined,
            error: undefined,
          }));
          return task;
        } catch (error) {
          set({
            actionTaskId: undefined,
            error: getAppErrorMessage(error, "Failed to run scheduled task."),
          });
          throw error;
        }
      },
      "Running task...",
    ),
  loadTaskRuns: async (taskId, limit = 25) => {
    set({ actionTaskId: taskId, error: undefined });
    try {
      const runs = await projectScheduledTaskApi.listRuns(taskId, limit);
      set((state) => ({
        runsByTaskId: {
          ...state.runsByTaskId,
          [taskId]: runs,
        },
        actionTaskId: undefined,
        error: undefined,
      }));
      return runs;
    } catch (error) {
      set({
        actionTaskId: undefined,
        error: getAppErrorMessage(error, "Failed to load scheduled task history."),
      });
      throw error;
    }
  },
  clearTaskHistory: async (taskId) =>
    runAsyncAction(
      `scheduled-task:${taskId}:clear-history`,
      async () => {
        set({ actionTaskId: taskId, error: undefined });
        try {
          const task = await projectScheduledTaskApi.clearHistory(taskId);
          set((state) => ({
            tasks: upsertTask(state.tasks, task),
            runsByTaskId: {
              ...state.runsByTaskId,
              [taskId]: [],
            },
            actionTaskId: undefined,
            error: undefined,
          }));
          return task;
        } catch (error) {
          set({
            actionTaskId: undefined,
            error: getAppErrorMessage(error, "Failed to clear scheduled task history."),
          });
          throw error;
        }
      },
      "Clearing task history...",
    ),
  removeTasksForProject: (projectId) =>
    set((state) => {
      const removedTaskIds = state.tasks
        .filter((task) => task.projectId === projectId)
        .map((task) => task.id);
      const nextRuns = { ...state.runsByTaskId };
      removedTaskIds.forEach((taskId) => {
        delete nextRuns[taskId];
      });
      return {
        tasks: state.tasks.filter((task) => task.projectId !== projectId),
        runsByTaskId: nextRuns,
      };
    }),
}));
