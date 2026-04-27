import type { Project } from "@/types/project";

export interface ProjectProfileCompatibilityWarning {
  code: string;
  title: string;
  message: string;
  suggestion?: string | null;
}

export interface ProjectProfileTransferResult {
  success: true;
  path: string;
  warnings: ProjectProfileCompatibilityWarning[];
}

export interface ImportedProjectProfile {
  project: Project;
  warnings: ProjectProfileCompatibilityWarning[];
}
