import type { Project } from "@/types/project";

export interface ProjectProfileTransferResult {
  success: true;
  path: string;
}

export interface ImportedProjectProfile {
  project: Project;
}
