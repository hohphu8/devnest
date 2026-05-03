import { create } from "zustand";
import { useProjectScheduledTaskStore } from "@/app/store/project-scheduled-task-store";
import { useProjectWorkerStore } from "@/app/store/project-worker-store";
import { runAsyncAction } from "@/app/store/async-action-store";
import { useProjectStore } from "@/app/store/project-store";
import { useServiceStore } from "@/app/store/service-store";
import { workspaceApi } from "@/lib/api/workspace-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type { WorkspaceOverviewPayload, WorkspacePortStatus } from "@/types/workspace";

interface RefreshOverviewOptions {
  silent?: boolean;
}

interface WorkspaceStore {
  overview?: WorkspaceOverviewPayload;
  loaded: boolean;
  loading: boolean;
  portSummaryLoading: boolean;
  error?: string;
  portSummaryError?: string;
  loadOverview: () => Promise<WorkspaceOverviewPayload>;
  refreshOverview: (options?: RefreshOverviewOptions) => Promise<WorkspaceOverviewPayload>;
  loadPortSummary: () => Promise<WorkspacePortStatus[]>;
}

let loadOverviewPromise: Promise<WorkspaceOverviewPayload> | undefined;
let loadPortSummaryPromise: Promise<WorkspacePortStatus[]> | undefined;

function hydrateStores(payload: WorkspaceOverviewPayload) {
  useProjectStore.getState().hydrateProjects(payload.projects);
  useServiceStore.getState().hydrateServices(payload.services);
  useProjectWorkerStore.getState().hydrateWorkers(payload.workers);
  useProjectScheduledTaskStore.getState().hydrateTasks(payload.scheduledTasks);
}

export const useWorkspaceStore = create<WorkspaceStore>((set, get) => ({
  overview: undefined,
  loaded: false,
  loading: false,
  portSummaryLoading: false,
  error: undefined,
  portSummaryError: undefined,
  loadOverview: async (): Promise<WorkspaceOverviewPayload> => {
    if (loadOverviewPromise) {
      return loadOverviewPromise;
    }

    set({ loading: true, error: undefined });
    loadOverviewPromise = (async () => {
      try {
        const overview = await workspaceApi.overview();
        hydrateStores(overview);
        set({
          overview,
          loaded: true,
          loading: false,
          portSummaryLoading: overview.portSummary.length === 0,
          error: undefined,
        });
        void get()
          .loadPortSummary()
          .catch(() => undefined);
        return overview;
      } catch (error) {
        const message = getAppErrorMessage(error, "Failed to load workspace overview.");
        set({
          loaded: false,
          loading: false,
          error: message,
        });
        throw error;
      } finally {
        loadOverviewPromise = undefined;
      }
    })();

    return loadOverviewPromise;
  },
  refreshOverview: async (options?: RefreshOverviewOptions): Promise<WorkspaceOverviewPayload> => {
    if (loadOverviewPromise) {
      return loadOverviewPromise;
    }

    const refresh = async () => {
      set((state) => ({ loading: options?.silent ? state.loading : true, error: undefined }));
      try {
        const overview = await workspaceApi.overview();
        hydrateStores(overview);
        set((state) => ({
          overview: {
            ...overview,
            portSummary:
              state.overview?.portSummary && state.overview.portSummary.length > 0
                ? state.overview.portSummary
                : overview.portSummary,
          },
          loaded: true,
          loading: false,
          portSummaryLoading:
            state.overview?.portSummary && state.overview.portSummary.length > 0
              ? state.portSummaryLoading
              : true,
          error: undefined,
        }));
        void get()
          .loadPortSummary()
          .catch(() => undefined);
        return overview;
      } catch (error) {
        const message = getAppErrorMessage(error, "Failed to refresh workspace overview.");
        set((state) => ({
          overview: state.overview,
          loaded: Boolean(state.overview),
          loading: false,
          error: message,
        }));
        throw error;
      } finally {
        loadOverviewPromise = undefined;
      }
    };

    loadOverviewPromise = options?.silent
      ? refresh()
      : runAsyncAction("workspace:refresh", refresh, "Refreshing workspace...");

    return loadOverviewPromise;
  },
  loadPortSummary: async (): Promise<WorkspacePortStatus[]> => {
    if (loadPortSummaryPromise) {
      return loadPortSummaryPromise;
    }

    set({ portSummaryLoading: true, portSummaryError: undefined });
    loadPortSummaryPromise = (async () => {
      try {
        const serviceSnapshot = get().overview?.services ?? useServiceStore.getState().services;
        const portSummary = await workspaceApi.portSummary(serviceSnapshot);
        set((state) => ({
          overview: state.overview ? { ...state.overview, portSummary } : state.overview,
          portSummaryLoading: false,
          portSummaryError: undefined,
        }));
        return portSummary;
      } catch (error) {
        const message = getAppErrorMessage(error, "Failed to load workspace port summary.");
        set({
          portSummaryLoading: false,
          portSummaryError: message,
        });
        throw error;
      } finally {
        loadPortSummaryPromise = undefined;
      }
    })();

    return loadPortSummaryPromise;
  },
}));
