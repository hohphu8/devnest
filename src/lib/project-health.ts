import type { DiagnosticItem } from "@/types/diagnostics";
import type { Project, ProjectStatus } from "@/types/project";
import type { ServiceState } from "@/types/service";

export interface DiagnosticSummary {
  errors: number;
  warnings: number;
  infos: number;
  suggestions: number;
  actionable: number;
}

export function getProjectService(project: Project, services: ServiceState[]): ServiceState | undefined {
  return services.find((service) => service.name === project.serverType);
}

export function getLiveProjectStatus(project: Project, services: ServiceState[]): ProjectStatus {
  const linkedService = getProjectService(project, services);

  if (linkedService?.status === "error") {
    return "error";
  }

  if (linkedService?.status === "running") {
    return "running";
  }

  return project.status === "error" ? "error" : "stopped";
}

export function getStatusTone(status: ProjectStatus): "success" | "warning" | "error" {
  if (status === "running") {
    return "success";
  }

  if (status === "error") {
    return "error";
  }

  return "warning";
}

export function summarizeDiagnostics(items: DiagnosticItem[]): DiagnosticSummary {
  const summary = {
    errors: 0,
    warnings: 0,
    infos: 0,
    suggestions: 0,
    actionable: 0,
  };

  items.forEach((item) => {
    if (item.level === "error") {
      summary.errors += 1;
      summary.actionable += 1;
    } else if (item.level === "warning") {
      summary.warnings += 1;
      summary.actionable += 1;
    } else {
      summary.infos += 1;
    }

    if (item.suggestion) {
      summary.suggestions += 1;
    }
  });

  return summary;
}
