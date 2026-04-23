import type { PropsWithChildren, ReactNode } from "react";
import type { IconName } from "@/components/ui/icons";
import { Icon } from "@/components/ui/icons";

interface EmptyStateProps {
  title: string;
  description: string;
  actions?: ReactNode;
  icon?: IconName;
}

export function EmptyState({
  title,
  description,
  actions,
  icon = "folderOpen",
}: PropsWithChildren<EmptyStateProps>) {
  return (
    <div className="empty-state">
      <div className="empty-state-mark" aria-hidden="true">
        <Icon name={icon} />
      </div>
      <div className="empty-state-copy">
        <h3>{title}</h3>
        <p>{description}</p>
      </div>
      {actions}
    </div>
  );
}
