import type { Project, ServerType } from "@/types/project";

export interface ScaffoldRecipeInput {
  path: string;
  domain: string;
  phpVersion: string;
  serverType: ServerType;
  sslEnabled: boolean;
}

export interface CloneGitRecipeInput extends ScaffoldRecipeInput {
  repositoryUrl: string;
  branch?: string | null;
}

export interface RecipeResult {
  project: Project;
}
