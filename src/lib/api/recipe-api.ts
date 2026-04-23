import { tauriInvoke } from "@/lib/tauri";
import type { Project } from "@/types/project";
import type { CloneGitRecipeInput, ScaffoldRecipeInput } from "@/types/recipe";

export const recipeApi = {
  createLaravel: (input: ScaffoldRecipeInput) =>
    tauriInvoke<Project>("create_laravel_recipe", { input }),
  createWordPress: (input: ScaffoldRecipeInput) =>
    tauriInvoke<Project>("create_wordpress_recipe", { input }),
  cloneGit: (input: CloneGitRecipeInput) =>
    tauriInvoke<Project>("clone_git_recipe", { input }),
};
