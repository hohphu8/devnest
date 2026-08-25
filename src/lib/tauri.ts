import { invoke } from "@tauri-apps/api/core";

export interface AppError {
  code: string;
  message: string;
  details?: unknown;
}

export function getAppErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }

  return fallback;
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function tauriInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauriRuntime()) {
    const { getMockResponseWithArgs } =
      await import("@/lib/tauri-browser-mock");
    return getMockResponseWithArgs<T>(command, args);
  }

  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const normalized =
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      "message" in error
        ? (error as AppError)
        : {
            code: "UNKNOWN_TAURI_ERROR",
            message:
              "An unexpected error occurred while invoking a native command.",
            details: error,
          };

    throw normalized;
  }
}
