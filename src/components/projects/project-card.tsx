import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ProjectMobilePreviewModal } from "@/components/projects/project-mobile-preview-modal";
import { useServiceStore } from "@/app/store/service-store";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icons";
import { mobilePreviewApi } from "@/lib/api/mobile-preview-api";
import { getLiveProjectStatus, getStatusTone } from "@/lib/project-health";
import type { ProjectMobilePreviewState } from "@/types/mobile-preview";
import type { Project } from "@/types/project";
import { useEffect } from "react";

interface ProjectCardProps {
  project: Project;
  issueCount?: number;
  onInspect?: (projectId: string) => void;
}

export function ProjectCard({ project, issueCount = 0, onInspect }: ProjectCardProps) {
  const navigate = useNavigate();
  const services = useServiceStore((state) => state.services);
  const liveStatus = getLiveProjectStatus(project, services);
  const [mobilePreviewOpen, setMobilePreviewOpen] = useState(false);
  const [mobilePreviewState, setMobilePreviewState] = useState<ProjectMobilePreviewState | null>(null);

  useEffect(() => {
    let cancelled = false;

    mobilePreviewApi
      .getState(project.id)
      .then((next) => {
        if (!cancelled) {
          setMobilePreviewState(next);
        }
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, [project.id]);

  return (
    <>
      <article className="project-card">
        <div className="project-card-head">
          <div className="project-card-title">
            <div className="project-card-mark" aria-hidden="true">
              <Icon name="folderOpen" />
            </div>
            <div className="project-card-copy">
              <h3>{project.name}</h3>
              <p>{project.domain}</p>
            </div>
          </div>
          <Button onClick={() => setMobilePreviewOpen(true)} size="sm">
            {mobilePreviewState?.status === "running" ? "Preview Running" : "Mobile Preview"}
          </Button>
        </div>

        <div className="project-card-badges">
          <Badge>{project.framework}</Badge>
          <Badge>{project.serverType}</Badge>
          <Badge>PHP {project.phpVersion}</Badge>
          {mobilePreviewState?.status === "running" ? <Badge>Preview On</Badge> : null}
        </div>

        <div className="project-card-health">
          <div className="status-chip" data-tone={getStatusTone(liveStatus)}>
            <Icon name={issueCount > 0 ? "alert" : liveStatus === "running" ? "activity" : "check"} />
            {liveStatus}
          </div>
          <span className="helper-text mono">{project.documentRoot}</span>
        </div>
        <div className="project-card-footer">
          <span className="helper-text">
            {issueCount > 0 ? `${issueCount} issues to review` : "No active issues"}
          </span>
          <div className="project-card-actions">
            <Button
              aria-label={`Open logs for ${project.name}`}
              onClick={() => navigate(`/logs?source=${project.serverType}`)}
              size="icon"
              title="Open logs"
              variant="ghost"
            >
              <Icon name="logs" />
            </Button>
            <Button onClick={() => (onInspect ? onInspect(project.id) : navigate(`/projects?projectId=${project.id}`))} variant="primary">
              <Icon name="eye" />
            Inspect
            </Button>
          </div>
        </div>
      </article>
      <ProjectMobilePreviewModal
        onClose={() => setMobilePreviewOpen(false)}
        onPreviewStateChange={setMobilePreviewState}
        open={mobilePreviewOpen}
        project={project}
      />
    </>
  );
}
