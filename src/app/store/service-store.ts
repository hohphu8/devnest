import { create } from "zustand";
import { runAsyncAction } from "@/app/store/async-action-store";
import { serviceActionLabel } from "@/lib/action-labels";
import { serviceApi } from "@/lib/api/service-api";
import { getAppErrorMessage } from "@/lib/tauri";
import type { ServiceName, ServiceState } from "@/types/service";

interface ServiceStore {
  services: ServiceState[];
  selectedServiceName?: ServiceName;
  activeService?: ServiceState;
  loaded: boolean;
  loading: boolean;
  actionName?: ServiceName;
  error?: string;
  hydrateServices: (services: ServiceState[]) => void;
  loadServices: () => Promise<void>;
  fetchService: (name: ServiceName) => Promise<ServiceState>;
  startService: (name: ServiceName) => Promise<ServiceState>;
  stopService: (name: ServiceName) => Promise<ServiceState>;
  restartService: (name: ServiceName) => Promise<ServiceState>;
  selectService: (name?: ServiceName) => void;
}

function upsertService(services: ServiceState[], nextService: ServiceState): ServiceState[] {
  const exists = services.some((service) => service.name === nextService.name);
  return exists
    ? services.map((service) => (service.name === nextService.name ? nextService : service))
    : [...services, nextService];
}

let loadServicesPromise: Promise<void> | undefined;

export const useServiceStore = create<ServiceStore>((set, get) => ({
  services: [],
  selectedServiceName: undefined,
  activeService: undefined,
  loaded: false,
  loading: false,
  actionName: undefined,
  error: undefined,
  hydrateServices: (services) =>
    set((state) => ({
      services,
      activeService: state.selectedServiceName
        ? services.find((service) => service.name === state.selectedServiceName)
        : state.activeService,
      loaded: true,
      loading: false,
      actionName: undefined,
      error: undefined,
    })),
  loadServices: async () => {
    if (loadServicesPromise) {
      return loadServicesPromise;
    }

    set({ loading: true, error: undefined });
    loadServicesPromise = (async () => {
      try {
        const services = await serviceApi.list();
        get().hydrateServices(services);
      } catch (error) {
        set({
          loaded: false,
          loading: false,
          actionName: undefined,
          error: getAppErrorMessage(error, "Failed to load services."),
        });
      } finally {
        loadServicesPromise = undefined;
      }
    })();

    return loadServicesPromise;
  },
  fetchService: async (name) => {
    set({ loading: true, error: undefined });
    try {
      const service = await serviceApi.get(name);
      set((state) => ({
        activeService:
          state.selectedServiceName === name
            ? service
            : state.activeService,
        services: upsertService(state.services, service),
        loading: false,
        actionName: undefined,
      }));
      return service;
    } catch (error) {
      set({
        loading: false,
        actionName: undefined,
        error: getAppErrorMessage(error, "Failed to load service."),
      });
      throw error;
    }
  },
  startService: async (name) => {
    return runAsyncAction(
      `service:${name}`,
      async () => {
        set({ actionName: name, error: undefined });
        try {
          const service = await serviceApi.start(name);
          set((state) => ({
            services: upsertService(state.services, service),
            activeService: state.selectedServiceName === service.name ? service : state.activeService,
            actionName: undefined,
          }));
          return service;
        } catch (error) {
          set({
            actionName: undefined,
            error: getAppErrorMessage(error, "Failed to start service."),
          });
          throw error;
        }
      },
      serviceActionLabel("start", name),
    );
  },
  stopService: async (name) => {
    return runAsyncAction(
      `service:${name}`,
      async () => {
        set({ actionName: name, error: undefined });
        try {
          const service = await serviceApi.stop(name);
          set((state) => ({
            services: upsertService(state.services, service),
            activeService: state.selectedServiceName === service.name ? service : state.activeService,
            actionName: undefined,
          }));
          return service;
        } catch (error) {
          set({
            actionName: undefined,
            error: getAppErrorMessage(error, "Failed to stop service."),
          });
          throw error;
        }
      },
      serviceActionLabel("stop", name),
    );
  },
  restartService: async (name) => {
    return runAsyncAction(
      `service:${name}`,
      async () => {
        set({ actionName: name, error: undefined });
        try {
          const service = await serviceApi.restart(name);
          set((state) => ({
            services: upsertService(state.services, service),
            activeService: state.selectedServiceName === service.name ? service : state.activeService,
            actionName: undefined,
          }));
          return service;
        } catch (error) {
          set({
            actionName: undefined,
            error: getAppErrorMessage(error, "Failed to restart service."),
          });
          throw error;
        }
      },
      serviceActionLabel("restart", name),
    );
  },
  selectService: (selectedServiceName) =>
    set((state) => ({
      selectedServiceName,
      activeService: selectedServiceName
        ? state.services.find((service) => service.name === selectedServiceName)
        : undefined,
    })),
}));
