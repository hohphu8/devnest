import { create } from "zustand";

interface AsyncActionEntry {
  label?: string;
}

interface AsyncActionStore {
  pending: Record<string, AsyncActionEntry>;
  start: (actionKey: string, label?: string) => boolean;
  finish: (actionKey: string) => void;
}

const activeActionPromises = new Map<string, Promise<unknown>>();

export const useAsyncActionStore = create<AsyncActionStore>((set) => ({
  pending: {},
  start: (actionKey, label) => {
    let started = false;

    set((state) => {
      if (state.pending[actionKey]) {
        return state;
      }

      started = true;
      return {
        pending: {
          ...state.pending,
          [actionKey]: {
            label,
          },
        },
      };
    });

    return started;
  },
  finish: (actionKey) =>
    set((state) => {
      if (!state.pending[actionKey]) {
        return state;
      }

      const pending = { ...state.pending };
      delete pending[actionKey];
      return { pending };
    }),
}));

export function getAsyncActionLabel(actionKey: string): string | undefined {
  return useAsyncActionStore.getState().pending[actionKey]?.label;
}

export function isAsyncActionPending(actionKey: string): boolean {
  return Boolean(useAsyncActionStore.getState().pending[actionKey]);
}

export function useAsyncActionPending(actionKey: string): boolean {
  return useAsyncActionStore((state) => Boolean(state.pending[actionKey]));
}

export function useAsyncActionLabel(actionKey: string): string | undefined {
  return useAsyncActionStore((state) => state.pending[actionKey]?.label);
}

export function runAsyncAction<T>(
  actionKey: string,
  run: () => Promise<T>,
  label?: string,
): Promise<T> {
  const existing = activeActionPromises.get(actionKey);
  if (existing) {
    return existing as Promise<T>;
  }

  const started = useAsyncActionStore.getState().start(actionKey, label);
  if (!started) {
    return (activeActionPromises.get(actionKey) ?? Promise.resolve(undefined)) as Promise<T>;
  }

  const promise = (async () => {
    try {
      return await run();
    } finally {
      activeActionPromises.delete(actionKey);
      useAsyncActionStore.getState().finish(actionKey);
    }
  })();

  activeActionPromises.set(actionKey, promise);
  return promise;
}
