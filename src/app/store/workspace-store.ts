import { create } from "zustand";
import { useProjectScheduledTaskStore } from "@/app/store/project-scheduled-task-store";
import { useProjectWorkerStore } from "@/app/store/project-worker-store";
import { runAsyncAction } from "@/app/store/async-action-store";
import { useProjectStore } from "@/app/store/project-store";
import { useServiceStore } from "@/app/store/service-store";
import { workspaceApi } from "@/lib/api/workspace-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type { WorkspaceOverviewPayload } from "@/types/workspace";

interface WorkspaceStore {
  overview?: WorkspaceOverviewPayload;
  loaded: boolean;
  loading: boolean;
  error?: string;
  loadOverview: () => Promise<WorkspaceOverviewPayload>;
  refreshOverview: () => Promise<WorkspaceOverviewPayload>;
}

let loadOverviewPromise: Promise<WorkspaceOverviewPayload> | undefined;

function hydrateStores(payload: WorkspaceOverviewPayload) {
  useProjectStore.getState().hydrateProjects(payload.projects);
  useServiceStore.getState().hydrateServices(payload.services);
  useProjectWorkerStore.getState().hydrateWorkers(payload.workers);
  useProjectScheduledTaskStore.getState().hydrateTasks(payload.scheduledTasks);
}

export const useWorkspaceStore = create<WorkspaceStore>((set) => ({
  overview: undefined,
  loaded: false,
  loading: false,
  error: undefined,
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
          error: undefined,
        });
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
  refreshOverview: async (): Promise<WorkspaceOverviewPayload> => {
    if (loadOverviewPromise) {
      return loadOverviewPromise;
    }

    loadOverviewPromise = runAsyncAction(
      "workspace:refresh",
      async () => {
        set({ loading: true, error: undefined });
        try {
          const overview = await workspaceApi.overview();
          hydrateStores(overview);
          set({
            overview,
            loaded: true,
            loading: false,
            error: undefined,
          });
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
      },
      "Refreshing workspace...",
    );

    return loadOverviewPromise;
  },
}));
