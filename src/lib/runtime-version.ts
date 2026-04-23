import type { RuntimeInventoryItem } from "@/types/runtime";

export function runtimeVersionFamily(version: string): string {
  const [major = "", minor = ""] = version.trim().split(".");
  if (!major || !minor) {
    return version.trim();
  }

  return `${major}.${minor}`;
}

export function runtimeVersionMatches(expected: string, actual: string): boolean {
  const left = expected.trim().toLowerCase();
  const right = actual.trim().toLowerCase();
  if (left === right) {
    return true;
  }

  return runtimeVersionFamily(left) === runtimeVersionFamily(right);
}

export function installedPhpVersionFamilies(runtimeInventory: RuntimeInventoryItem[]): string[] {
  return Array.from(
    new Set(
      runtimeInventory
        .filter((runtime) => runtime.runtimeType === "php")
        .map((runtime) => runtimeVersionFamily(runtime.version))
        .filter(Boolean),
    ),
  ).sort((left, right) => left.localeCompare(right, undefined, { numeric: true }));
}
