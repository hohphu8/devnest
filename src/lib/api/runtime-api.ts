import { tauriInvoke } from "@/lib/tauri";
import type { RuntimeConfigSchema, RuntimeConfigValues } from "@/types/runtime-config";
import type {
  PhpExtensionInstallResult,
  PhpExtensionPackage,
  PhpExtensionState,
  PhpFunctionState,
  RuntimeInstallTask,
  RuntimeInventoryItem,
  RuntimePackage,
} from "@/types/runtime";

export const runtimeApi = {
  list: () => tauriInvoke<RuntimeInventoryItem[]>("list_runtime_inventory"),
  listPackages: () => tauriInvoke<RuntimePackage[]>("list_runtime_packages"),
  getConfigSchema: (runtimeId: string) =>
    tauriInvoke<RuntimeConfigSchema>("get_runtime_config_schema", { runtimeId }),
  getConfigValues: (runtimeId: string) =>
    tauriInvoke<RuntimeConfigValues>("get_runtime_config_values", { runtimeId }),
  updateConfig: (runtimeId: string, patch: Record<string, string>) =>
    tauriInvoke<RuntimeConfigValues>("update_runtime_config", { runtimeId, patch }),
  openConfigFile: (runtimeId: string) =>
    tauriInvoke<boolean>("open_runtime_config_file", { runtimeId }),
  verify: (runtimeType: string, path: string) =>
    tauriInvoke<RuntimeInventoryItem>("verify_runtime_path", { runtimeType, path }),
  link: (runtimeType: string, path: string, setActive = true) =>
    tauriInvoke<RuntimeInventoryItem>("link_runtime_path", { runtimeType, path, setActive }),
  import: (runtimeType: string, path: string, setActive = true) =>
    tauriInvoke<RuntimeInventoryItem>("import_runtime_path", { runtimeType, path, setActive }),
  installPackage: (packageId: string, setActive = true) =>
    tauriInvoke<RuntimeInventoryItem>("install_runtime_package", { packageId, setActive }),
  getInstallTask: () => tauriInvoke<RuntimeInstallTask | null>("get_runtime_install_task"),
  listPhpExtensions: (runtimeId: string) =>
    tauriInvoke<PhpExtensionState[]>("list_php_extensions", { runtimeId }),
  listPhpExtensionPackages: (runtimeId: string) =>
    tauriInvoke<PhpExtensionPackage[]>("list_php_extension_packages", { runtimeId }),
  setPhpExtensionEnabled: (runtimeId: string, extensionName: string, enabled: boolean) =>
    tauriInvoke<PhpExtensionState>("set_php_extension_enabled", {
      runtimeId,
      extensionName,
      enabled,
    }),
  installPhpExtension: (runtimeId: string) =>
    tauriInvoke<PhpExtensionInstallResult | null>("install_php_extension", { runtimeId }),
  installPhpExtensionPackage: (runtimeId: string, packageId: string) =>
    tauriInvoke<PhpExtensionInstallResult>("install_php_extension_package", {
      runtimeId,
      packageId,
    }),
  removePhpExtension: (runtimeId: string, extensionName: string) =>
    tauriInvoke<boolean>("remove_php_extension", { runtimeId, extensionName }),
  listPhpFunctions: (runtimeId: string) =>
    tauriInvoke<PhpFunctionState[]>("list_php_functions", { runtimeId }),
  setPhpFunctionEnabled: (runtimeId: string, functionName: string, enabled: boolean) =>
    tauriInvoke<PhpFunctionState>("set_php_function_enabled", {
      runtimeId,
      functionName,
      enabled,
    }),
  setActive: (runtimeId: string) =>
    tauriInvoke<RuntimeInventoryItem>("set_active_runtime", { runtimeId }),
  remove: (runtimeId: string) =>
    tauriInvoke<boolean>("remove_runtime_reference", { runtimeId }),
  reveal: (runtimeId: string) =>
    tauriInvoke<boolean>("reveal_runtime_path", { runtimeId }),
};
