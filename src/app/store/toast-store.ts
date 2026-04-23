import { create } from "zustand";

export type ToastTone = "success" | "error" | "warning" | "info";

export interface ToastItem {
  id: string;
  tone: ToastTone;
  title?: string;
  message: string;
  createdAt: string;
}

interface ToastStore {
  items: ToastItem[];
  push: (input: Omit<ToastItem, "id" | "createdAt"> & { durationMs?: number }) => string;
  dismiss: (id: string) => void;
  clear: () => void;
}

const toastTimers = new Map<string, ReturnType<typeof setTimeout>>();

function nextToastId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `toast-${Date.now()}-${Math.round(Math.random() * 1_000_000)}`;
}

export const useToastStore = create<ToastStore>((set) => ({
  items: [],
  push: ({ durationMs = 3600, ...input }) => {
    const id = nextToastId();
    const item: ToastItem = {
      id,
      createdAt: new Date().toISOString(),
      ...input,
    };

    set((state) => ({
      items: [item, ...state.items].slice(0, 5),
    }));

    const timer = setTimeout(() => {
      set((state) => ({
        items: state.items.filter((toast) => toast.id !== id),
      }));
      toastTimers.delete(id);
    }, durationMs);

    toastTimers.set(id, timer);
    return id;
  },
  dismiss: (id) => {
    const timer = toastTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      toastTimers.delete(id);
    }

    set((state) => ({
      items: state.items.filter((toast) => toast.id !== id),
    }));
  },
  clear: () => {
    toastTimers.forEach((timer) => clearTimeout(timer));
    toastTimers.clear();
    set({ items: [] });
  },
}));
