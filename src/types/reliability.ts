import type { DiagnosticItem } from "@/types/diagnostics";
import type { PersistentTunnelHealthReport, ProjectPersistentHostname, ProjectPersistentTunnelState } from "@/types/persistent-tunnel";
import type { Project } from "@/types/project";
import type { ServiceState } from "@/types/service";
import type { ProjectTunnelState } from "@/types/tunnel";

export type ReliabilityLayer =
  | "project"
  | "runtime"
  | "config"
  | "service"
  | "dns"
  | "tunnel"
  | "workspace";

export type ReliabilityStatus = "ok" | "warning" | "error";

export type ReliabilityAction =
  | "provisionProject"
  | "publishPersistentDomain"
  | "startProjectRuntime"
  | "restoreAppMetadata";

export type RepairWorkflow = "project" | "tunnel" | "runtimeLinks";

export interface ReliabilityCheck {
  code: string;
  layer: ReliabilityLayer;
  status: ReliabilityStatus;
  blocking: boolean;
  title: string;
  message: string;
  suggestion?: string | null;
}

export interface ActionPreflightReport {
  action: ReliabilityAction;
  projectId?: string | null;
  ready: boolean;
  summary: string;
  checks: ReliabilityCheck[];
  generatedAt: string;
}

export interface RepairWorkflowInfo {
  workflow: RepairWorkflow;
  title: string;
  summary: string;
  touches: string[];
}

export interface RepairExecutionResult {
  workflow: RepairWorkflow;
  success: true;
  message: string;
  touchedLayers: ReliabilityLayer[];
  generatedAt: string;
}

export interface InspectorConfigSnapshot {
  serverType: Project["serverType"];
  outputPath: string;
  preview?: string | null;
  localDomainAliasPresent: boolean;
  persistentAliasPresent: boolean;
}

export interface InspectorRuntimeBinding {
  kind: string;
  version?: string | null;
  path?: string | null;
  active: boolean;
  available: boolean;
  details?: string | null;
}

export interface InspectorRuntimeSnapshot {
  server: InspectorRuntimeBinding;
  php: InspectorRuntimeBinding;
  mysql?: InspectorRuntimeBinding | null;
  issues: string[];
}

export interface ReliabilityInspectorSnapshot {
  project: Project;
  diagnostics: DiagnosticItem[];
  services: ServiceState[];
  config: InspectorConfigSnapshot;
  runtime: InspectorRuntimeSnapshot;
  quickTunnel?: ProjectTunnelState | null;
  persistentHostname?: ProjectPersistentHostname | null;
  persistentTunnel?: ProjectPersistentTunnelState | null;
  persistentHealth?: PersistentTunnelHealthReport | null;
  generatedAt: string;
}

export interface ReliabilityTransferResult {
  success: true;
  path: string;
}
