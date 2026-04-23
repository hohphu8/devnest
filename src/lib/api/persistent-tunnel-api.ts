import { tauriInvoke } from "@/lib/tauri";
import type {
  ApplyProjectPersistentHostnameInput,
  ApplyProjectPersistentHostnameResult,
  CreatePersistentNamedTunnelInput,
  DeleteProjectPersistentHostnameResult,
  PersistentTunnelHealthReport,
  PersistentTunnelNamedTunnelSummary,
  PersistentTunnelSetupStatus,
  ProjectPersistentHostname,
  ProjectPersistentTunnelState,
  SelectPersistentNamedTunnelInput,
  UpdatePersistentTunnelSetupInput,
  UpsertProjectPersistentHostnameInput,
} from "@/types/persistent-tunnel";

export const persistentTunnelApi = {
  getSetupStatus: () =>
    tauriInvoke<PersistentTunnelSetupStatus>("get_persistent_tunnel_setup_status"),
  connectProvider: () =>
    tauriInvoke<PersistentTunnelSetupStatus>("connect_persistent_tunnel_provider"),
  importAuthCert: () =>
    tauriInvoke<PersistentTunnelSetupStatus | null>("import_persistent_tunnel_auth_cert"),
  createNamedTunnel: (input: CreatePersistentNamedTunnelInput) =>
    tauriInvoke<PersistentTunnelSetupStatus>("create_persistent_named_tunnel", {
      input,
    }),
  importCredentials: () =>
    tauriInvoke<PersistentTunnelSetupStatus | null>("import_persistent_tunnel_credentials"),
  listNamedTunnels: () =>
    tauriInvoke<PersistentTunnelNamedTunnelSummary[]>(
      "list_available_persistent_named_tunnels",
    ),
  selectNamedTunnel: (input: SelectPersistentNamedTunnelInput) =>
    tauriInvoke<PersistentTunnelSetupStatus>("select_persistent_named_tunnel", {
      input,
    }),
  deleteNamedTunnel: (tunnelId: string) =>
    tauriInvoke<PersistentTunnelSetupStatus>("delete_persistent_named_tunnel", {
      tunnelId,
    }),
  disconnectProvider: () =>
    tauriInvoke<PersistentTunnelSetupStatus>("disconnect_persistent_tunnel_provider"),
  updateSetup: (input: UpdatePersistentTunnelSetupInput) =>
    tauriInvoke<PersistentTunnelSetupStatus>("update_persistent_tunnel_setup", {
      input,
    }),
  getProjectHostname: (projectId: string) =>
    tauriInvoke<ProjectPersistentHostname | null>("get_project_persistent_hostname", {
      projectId,
    }),
  applyProjectHostname: (input: ApplyProjectPersistentHostnameInput) =>
    tauriInvoke<ApplyProjectPersistentHostnameResult>("apply_project_persistent_hostname", {
      input,
    }),
  upsertProjectHostname: (input: UpsertProjectPersistentHostnameInput) =>
    tauriInvoke<ProjectPersistentHostname>("upsert_project_persistent_hostname", {
      input,
    }),
  deleteProjectHostname: (projectId: string) =>
    tauriInvoke<DeleteProjectPersistentHostnameResult>("delete_project_persistent_hostname", {
      projectId,
    }),
  removeProjectHostname: (projectId: string) =>
    tauriInvoke<boolean>("remove_project_persistent_hostname", { projectId }),
  getProjectTunnelState: (projectId: string) =>
    tauriInvoke<ProjectPersistentTunnelState | null>("get_project_persistent_tunnel_state", {
      projectId,
    }),
  publishProjectTunnel: (projectId: string) =>
    tauriInvoke<ProjectPersistentTunnelState>("publish_project_persistent_tunnel", {
      projectId,
    }),
  startProjectTunnel: (projectId: string) =>
    tauriInvoke<ProjectPersistentTunnelState>("start_project_persistent_tunnel", {
      projectId,
    }),
  stopProjectTunnel: (projectId: string) =>
    tauriInvoke<ProjectPersistentTunnelState>("stop_project_persistent_tunnel", {
      projectId,
    }),
  unpublishProjectTunnel: (projectId: string) =>
    tauriInvoke<ProjectPersistentTunnelState>("unpublish_project_persistent_tunnel", {
      projectId,
    }),
  openProjectTunnel: (projectId: string) =>
    tauriInvoke<boolean>("open_project_persistent_tunnel_url", { projectId }),
  inspectProjectHealth: (projectId: string) =>
    tauriInvoke<PersistentTunnelHealthReport>("inspect_project_persistent_tunnel_health", {
      projectId,
    }),
};
