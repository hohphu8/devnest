import { create } from "zustand";
import { runAsyncAction } from "@/app/store/async-action-store";
import { useProjectScheduledTaskStore } from "@/app/store/project-scheduled-task-store";
import { useProjectWorkerStore } from "@/app/store/project-worker-store";
import { projectApi } from "@/lib/api/project-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type { CreateProjectInput, Project, UpdateProjectPatch } from "@/types/project";

interface ProjectStore {
  projects: Project[];
  selectedProjectId?: string;
  activeProject?: Project;
  loaded: boolean;
  loading: boolean;
  error?: string;
  hydrateProjects: (projects: Project[]) => void;
  loadProjects: () => Promise<void>;
  fetchProject: (projectId: string) => Promise<Project>;
  createProject: (input: CreateProjectInput) => Promise<Project>;
  updateProject: (projectId: string, patch: UpdateProjectPatch) => Promise<Project>;
  deleteProject: (projectId: string) => Promise<void>;
  selectProject: (projectId?: string) => void;
}

let loadProjectsPromise: Promise<void> | undefined;

export const useProjectStore = create<ProjectStore>((set, get) => ({
  projects: [],
  loaded: false,
  loading: false,
  error: undefined,
  activeProject: undefined,
  hydrateProjects: (projects) =>
    set((state) => ({
      projects,
      activeProject: state.selectedProjectId
        ? projects.find((project) => project.id === state.selectedProjectId)
        : state.activeProject,
      loaded: true,
      loading: false,
      error: undefined,
    })),
  loadProjects: async () => {
    if (loadProjectsPromise) {
      return loadProjectsPromise;
    }

    set({ loading: true, error: undefined });
    loadProjectsPromise = (async () => {
      try {
        const projects = await projectApi.list();
        get().hydrateProjects(projects);
      } catch (error) {
        set({
          loaded: false,
          loading: false,
          error: getAppErrorMessage(error, "Failed to load projects."),
        });
      } finally {
        loadProjectsPromise = undefined;
      }
    })();

    return loadProjectsPromise;
  },
  fetchProject: async (projectId) => {
    set({ loading: true, error: undefined });
    try {
      const project = await projectApi.get(projectId);
      set((state) => ({
        activeProject: project,
        selectedProjectId: project.id,
        projects: state.projects.map((item) => (item.id === project.id ? project : item)),
        loading: false,
      }));
      return project;
    } catch (error) {
      set({ loading: false, error: getAppErrorMessage(error, "Failed to load project.") });
      throw error;
    }
  },
  createProject: async (input) => {
    return runAsyncAction(
      `project:create:${input.path.trim().toLowerCase()}`,
      async () => {
        set({ loading: true, error: undefined });
        try {
          const project = await projectApi.create(input);
          set((state) => ({
            projects: [project, ...state.projects],
            selectedProjectId: project.id,
            activeProject: project,
            loading: false,
          }));
          return project;
        } catch (error) {
          set({ loading: false, error: getAppErrorMessage(error, "Failed to create project.") });
          throw error;
        }
      },
      `Creating ${input.name || "project"}...`,
    );
  },
  updateProject: async (projectId, patch) => {
    return runAsyncAction(
      `project:${projectId}:save`,
      async () => {
        set({ loading: true, error: undefined });
        try {
          const updatedProject = await projectApi.update(projectId, patch);
          set((state) => ({
            projects: state.projects.map((project) =>
              project.id === updatedProject.id ? updatedProject : project,
            ),
            activeProject:
              state.activeProject?.id === updatedProject.id ? updatedProject : state.activeProject,
            loading: false,
          }));
          return updatedProject;
        } catch (error) {
          set({ loading: false, error: getAppErrorMessage(error, "Failed to update project.") });
          throw error;
        }
      },
      "Saving project...",
    );
  },
  deleteProject: async (projectId) => {
    return runAsyncAction(
      `project:${projectId}:delete`,
      async () => {
        set({ loading: true, error: undefined });
        try {
          await projectApi.remove(projectId);
          useProjectWorkerStore.getState().removeWorkersForProject(projectId);
          useProjectScheduledTaskStore.getState().removeTasksForProject(projectId);
          set((state) => ({
            projects: state.projects.filter((project) => project.id !== projectId),
            activeProject: state.activeProject?.id === projectId ? undefined : state.activeProject,
            selectedProjectId:
              state.selectedProjectId === projectId ? undefined : state.selectedProjectId,
            loading: false,
          }));
        } catch (error) {
          set({ loading: false, error: getAppErrorMessage(error, "Failed to delete project.") });
          throw error;
        }
      },
      "Deleting project...",
    );
  },
  selectProject: (selectedProjectId) =>
    set((state) => ({
      selectedProjectId,
      activeProject: selectedProjectId
        ? state.projects.find((project) => project.id === selectedProjectId)
        : undefined,
    })),
}));
