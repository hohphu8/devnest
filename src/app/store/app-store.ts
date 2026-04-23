import { create } from "zustand";

const SIDEBAR_COLLAPSED_STORAGE_KEY = "devnest.sidebar.collapsed";

interface AppStore {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
}

function readSidebarCollapsedPreference() {
  if (typeof window === "undefined") {
    return false;
  }

  return window.localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY) === "1";
}

export const useAppStore = create<AppStore>((set) => ({
  sidebarCollapsed: readSidebarCollapsedPreference(),
  toggleSidebar: () =>
    set((state) => {
      const sidebarCollapsed = !state.sidebarCollapsed;
      if (typeof window !== "undefined") {
        window.localStorage.setItem(
          SIDEBAR_COLLAPSED_STORAGE_KEY,
          sidebarCollapsed ? "1" : "0",
        );
      }

      return { sidebarCollapsed };
    }),
}));
