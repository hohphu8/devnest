import { useEffect, type PropsWithChildren } from "react";
import { useWorkspaceStore } from "@/app/store/workspace-store";

export function AppProviders({ children }: PropsWithChildren) {
  const workspaceLoaded = useWorkspaceStore((state) => state.loaded);
  const loadOverview = useWorkspaceStore((state) => state.loadOverview);

  useEffect(() => {
    if (!workspaceLoaded) {
      void loadOverview().catch(() => undefined);
    }
  }, [loadOverview, workspaceLoaded]);

  return children;
}
