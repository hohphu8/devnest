import { tauriInvoke } from "@/lib/tauri";
import type { ServerType } from "@/types/project";

export interface PreviewVhostConfigResult {
  serverType: ServerType;
  configText: string;
  outputPath: string;
}

export interface GenerateVhostConfigResult {
  success: true;
  outputPath: string;
}

export interface ProjectSslResult {
  success: true;
  domain: string;
  certPath: string;
  keyPath: string;
}

export interface LocalSslAuthorityResult {
  success: true;
  certPath: string;
  trusted: boolean;
}

export interface ApplyHostsEntryResult {
  success: true;
  domain: string;
  targetIp: string;
}

export const configApi = {
  preview: (projectId: string) =>
    tauriInvoke<PreviewVhostConfigResult>("preview_vhost_config", { projectId }),
  generate: (projectId: string) =>
    tauriInvoke<GenerateVhostConfigResult>("generate_vhost_config", { projectId }),
  trustSslAuthority: () =>
    tauriInvoke<LocalSslAuthorityResult>("trust_local_ssl_authority"),
  getSslAuthorityStatus: () =>
    tauriInvoke<LocalSslAuthorityResult>("get_local_ssl_authority_status"),
  untrustSslAuthority: () =>
    tauriInvoke<LocalSslAuthorityResult>("untrust_local_ssl_authority"),
  regenerateSsl: (projectId: string) =>
    tauriInvoke<ProjectSslResult>("regenerate_project_ssl_certificate", { projectId }),
  openSite: (projectId: string, preferHttps = false) =>
    tauriInvoke<true>("open_project_site", { projectId, preferHttps }),
  applyHosts: (domain: string, targetIp?: string) =>
    tauriInvoke<ApplyHostsEntryResult>("apply_hosts_entry", { domain, targetIp }),
  removeHosts: (domain: string) =>
    tauriInvoke<{ success: true }>("remove_hosts_entry", { domain }),
};
