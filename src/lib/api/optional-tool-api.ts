import { tauriInvoke } from "@/lib/tauri";
import type {
  OptionalToolInstallTask,
  OptionalToolInventoryItem,
  OptionalToolPackage,
} from "@/types/optional-tool";

export const optionalToolApi = {
  list: () => tauriInvoke<OptionalToolInventoryItem[]>("list_optional_tool_inventory"),
  listPackages: () => tauriInvoke<OptionalToolPackage[]>("list_optional_tool_packages"),
  installPackage: (packageId: string) =>
    tauriInvoke<OptionalToolInventoryItem>("install_optional_tool_package", { packageId }),
  getInstallTask: () =>
    tauriInvoke<OptionalToolInstallTask | null>("get_optional_tool_install_task"),
  remove: (toolId: string) => tauriInvoke<boolean>("remove_optional_tool", { toolId }),
  reveal: (toolId: string) => tauriInvoke<boolean>("reveal_optional_tool_path", { toolId }),
};
