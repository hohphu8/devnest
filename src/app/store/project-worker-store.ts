import { create } from "zustand";
import { runAsyncAction } from "@/app/store/async-action-store";
import { projectWorkerApi } from "@/lib/api/project-worker-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type {
  CreateProjectWorkerInput,
  ProjectWorker,
  UpdateProjectWorkerPatch,
} from "@/types/project-worker";

interface ProjectWorkerStore {
  workers: ProjectWorker[];
  loaded: boolean;
  loading: boolean;
  actionWorkerId?: string;
  error?: string;
  hydrateWorkers: (workers: ProjectWorker[]) => void;
  loadWorkers: () => Promise<void>;
  loadProjectWorkers: (projectId: string) => Promise<ProjectWorker[]>;
  fetchWorkerStatus: (workerId: string) => Promise<ProjectWorker>;
  createWorker: (input: CreateProjectWorkerInput) => Promise<ProjectWorker>;
  updateWorker: (workerId: string, patch: UpdateProjectWorkerPatch) => Promise<ProjectWorker>;
  deleteWorker: (workerId: string) => Promise<void>;
  startWorker: (workerId: string) => Promise<ProjectWorker>;
  stopWorker: (workerId: string) => Promise<ProjectWorker>;
  restartWorker: (workerId: string) => Promise<ProjectWorker>;
  removeWorkersForProject: (projectId: string) => void;
}

function upsertWorker(workers: ProjectWorker[], nextWorker: ProjectWorker): ProjectWorker[] {
  const exists = workers.some((worker) => worker.id === nextWorker.id);
  return exists
    ? workers.map((worker) => (worker.id === nextWorker.id ? nextWorker : worker))
    : [nextWorker, ...workers];
}

let loadWorkersPromise: Promise<void> | undefined;

export const useProjectWorkerStore = create<ProjectWorkerStore>((set, get) => ({
  workers: [],
  loaded: false,
  loading: false,
  actionWorkerId: undefined,
  error: undefined,
  hydrateWorkers: (workers) =>
    set({
      workers,
      loaded: true,
      loading: false,
      actionWorkerId: undefined,
      error: undefined,
    }),
  loadWorkers: async () => {
    if (loadWorkersPromise) {
      return loadWorkersPromise;
    }

    set({ loading: true, error: undefined });
    loadWorkersPromise = (async () => {
      try {
        get().hydrateWorkers(await projectWorkerApi.listAll());
      } catch (error) {
        set({
          loaded: false,
          loading: false,
          actionWorkerId: undefined,
          error: getAppErrorMessage(error, "Failed to load workers."),
        });
      } finally {
        loadWorkersPromise = undefined;
      }
    })();

    return loadWorkersPromise;
  },
  loadProjectWorkers: async (projectId) => {
    set({ loading: true, error: undefined });
    try {
      const projectWorkers = await projectWorkerApi.listByProject(projectId);
      set((state) => ({
        workers: [
          ...projectWorkers,
          ...state.workers.filter((worker) => worker.projectId !== projectId),
        ],
        loaded: true,
        loading: false,
        actionWorkerId: undefined,
        error: undefined,
      }));
      return projectWorkers;
    } catch (error) {
      set({
        loading: false,
        actionWorkerId: undefined,
        error: getAppErrorMessage(error, "Failed to load project workers."),
      });
      throw error;
    }
  },
  fetchWorkerStatus: async (workerId) => {
    set({ loading: true, error: undefined });
    try {
      const worker = await projectWorkerApi.getStatus(workerId);
      set((state) => ({
        workers: upsertWorker(state.workers, worker),
        loaded: true,
        loading: false,
        actionWorkerId: undefined,
        error: undefined,
      }));
      return worker;
    } catch (error) {
      set({
        loading: false,
        actionWorkerId: undefined,
        error: getAppErrorMessage(error, "Failed to load worker status."),
      });
      throw error;
    }
  },
  createWorker: async (input) =>
    runAsyncAction(
      `worker:create:${input.projectId}:${input.name.trim().toLowerCase()}`,
      async () => {
        set({ loading: true, error: undefined });
        try {
          const worker = await projectWorkerApi.create(input);
          set((state) => ({
            workers: upsertWorker(state.workers, worker),
            loaded: true,
            loading: false,
            error: undefined,
          }));
          return worker;
        } catch (error) {
          set({
            loading: false,
            error: getAppErrorMessage(error, "Failed to create worker."),
          });
          throw error;
        }
      },
      "Creating worker...",
    ),
  updateWorker: async (workerId, patch) =>
    runAsyncAction(
      `worker:${workerId}:save`,
      async () => {
        set({ actionWorkerId: workerId, error: undefined });
        try {
          const worker = await projectWorkerApi.update(workerId, patch);
          set((state) => ({
            workers: upsertWorker(state.workers, worker),
            actionWorkerId: undefined,
            error: undefined,
          }));
          return worker;
        } catch (error) {
          set({
            actionWorkerId: undefined,
            error: getAppErrorMessage(error, "Failed to update worker."),
          });
          throw error;
        }
      },
      "Saving worker...",
    ),
  deleteWorker: async (workerId) =>
    runAsyncAction(
      `worker:${workerId}:delete`,
      async () => {
        set({ actionWorkerId: workerId, error: undefined });
        try {
          await projectWorkerApi.remove(workerId);
          set((state) => ({
            workers: state.workers.filter((worker) => worker.id !== workerId),
            actionWorkerId: undefined,
            error: undefined,
          }));
        } catch (error) {
          set({
            actionWorkerId: undefined,
            error: getAppErrorMessage(error, "Failed to delete worker."),
          });
          throw error;
        }
      },
      "Deleting worker...",
    ),
  startWorker: async (workerId) =>
    runAsyncAction(
      `worker:${workerId}:start`,
      async () => {
        set({ actionWorkerId: workerId, error: undefined });
        try {
          const worker = await projectWorkerApi.start(workerId);
          set((state) => ({
            workers: upsertWorker(state.workers, worker),
            actionWorkerId: undefined,
            error: undefined,
          }));
          return worker;
        } catch (error) {
          set({
            actionWorkerId: undefined,
            error: getAppErrorMessage(error, "Failed to start worker."),
          });
          throw error;
        }
      },
      "Starting worker...",
    ),
  stopWorker: async (workerId) =>
    runAsyncAction(
      `worker:${workerId}:stop`,
      async () => {
        set({ actionWorkerId: workerId, error: undefined });
        try {
          const worker = await projectWorkerApi.stop(workerId);
          set((state) => ({
            workers: upsertWorker(state.workers, worker),
            actionWorkerId: undefined,
            error: undefined,
          }));
          return worker;
        } catch (error) {
          set({
            actionWorkerId: undefined,
            error: getAppErrorMessage(error, "Failed to stop worker."),
          });
          throw error;
        }
      },
      "Stopping worker...",
    ),
  restartWorker: async (workerId) =>
    runAsyncAction(
      `worker:${workerId}:restart`,
      async () => {
        set({ actionWorkerId: workerId, error: undefined });
        try {
          const worker = await projectWorkerApi.restart(workerId);
          set((state) => ({
            workers: upsertWorker(state.workers, worker),
            actionWorkerId: undefined,
            error: undefined,
          }));
          return worker;
        } catch (error) {
          set({
            actionWorkerId: undefined,
            error: getAppErrorMessage(error, "Failed to restart worker."),
          });
          throw error;
        }
      },
      "Restarting worker...",
    ),
  removeWorkersForProject: (projectId) =>
    set((state) => ({
      workers: state.workers.filter((worker) => worker.projectId !== projectId),
    })),
}));
