import { create } from "zustand";
import { diagnosticsApi } from "@/lib/api/diagnostics-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type { DiagnosticItem } from "@/types/diagnostics";

interface DiagnosticsStore {
  itemsByProject: Record<string, DiagnosticItem[]>;
  lastRunAtByProject: Record<string, string>;
  loadingProjectId?: string;
  error?: string;
  runDiagnostics: (projectId: string) => Promise<DiagnosticItem[]>;
}

export const useDiagnosticsStore = create<DiagnosticsStore>((set) => ({
  itemsByProject: {},
  lastRunAtByProject: {},
  loadingProjectId: undefined,
  error: undefined,
  runDiagnostics: async (projectId) => {
    set({ loadingProjectId: projectId, error: undefined });
    try {
      const items = await diagnosticsApi.run(projectId);
      const lastRunAt = new Date().toISOString();
      set((state) => ({
        itemsByProject: {
          ...state.itemsByProject,
          [projectId]: items,
        },
        lastRunAtByProject: {
          ...state.lastRunAtByProject,
          [projectId]: lastRunAt,
        },
        loadingProjectId: undefined,
      }));
      return items;
    } catch (error) {
      set({
        loadingProjectId: undefined,
        error: getAppErrorMessage(error, "Failed to run diagnostics."),
      });
      throw error;
    }
  },
}));
