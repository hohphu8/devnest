import { useProjectStore } from "@/app/store/project-store";
import { useProjectScheduledTaskStore } from "@/app/store/project-scheduled-task-store";
import { useProjectWorkerStore } from "@/app/store/project-worker-store";
import { useServiceStore } from "@/app/store/service-store";
import { Icon } from "@/components/ui/icons";
import { getLiveProjectStatus } from "@/lib/project-health";

export function StatusStrip() {
  const projectsLoaded = useProjectStore((state) => state.loaded);
  const projectsLoading = useProjectStore((state) => state.loading);
  const projects = useProjectStore((state) => state.projects);
  const projectError = useProjectStore((state) => state.error);
  const servicesLoaded = useServiceStore((state) => state.loaded);
  const servicesLoading = useServiceStore((state) => state.loading);
  const services = useServiceStore((state) => state.services);
  const serviceError = useServiceStore((state) => state.error);
  const workersLoaded = useProjectWorkerStore((state) => state.loaded);
  const workersLoading = useProjectWorkerStore((state) => state.loading);
  const workers = useProjectWorkerStore((state) => state.workers);
  const workerError = useProjectWorkerStore((state) => state.error);
  const tasksLoaded = useProjectScheduledTaskStore((state) => state.loaded);
  const tasksLoading = useProjectScheduledTaskStore((state) => state.loading);
  const tasks = useProjectScheduledTaskStore((state) => state.tasks);
  const taskError = useProjectScheduledTaskStore((state) => state.error);

  const runningServices = services.filter((service) => service.status === "running").length;
  const runningWorkers = workers.filter((worker) => worker.status === "running").length;
  const enabledTasks = tasks.filter((task) => task.enabled).length;
  const projectAlerts = projects.filter((project) => getLiveProjectStatus(project, services) === "error").length;
  const workspaceLoading =
    !projectsLoaded ||
    !servicesLoaded ||
    !workersLoaded ||
    !tasksLoaded ||
    projectsLoading ||
    servicesLoading ||
    workersLoading ||
    tasksLoading;
  const workspaceError = projectError ?? serviceError ?? workerError ?? taskError;
  const workspaceStatus = workspaceError
    ? "Workspace needs review"
    : workspaceLoading
      ? "Loading workspace"
      : runningServices > 0
        ? "Workspace active"
        : "Workspace idle";

  return (
    <footer className="status-strip">
      <div className="status-group">
        <Icon name={!workspaceLoading && !workspaceError && runningServices > 0 ? "activity" : workspaceError ? "alert" : "check"} />
        <span
          className={`status-pip ${
            !workspaceLoading && !workspaceError && runningServices > 0 ? "status-pip-live" : ""
          }`}
        />
        <span>{workspaceStatus}</span>
      </div>
      <div className="status-group">
        <Icon name="folder" />
        <span className="status-label">Projects</span>
        <strong>{projects.length} tracked</strong>
      </div>
      <div className="status-group">
        <Icon name="server" />
        <span className="status-label">Services</span>
        <strong>{runningServices} running</strong>
      </div>
      <div className="status-group">
        <Icon name="activity" />
        <span className="status-label">Workers</span>
        <strong>{runningWorkers} running</strong>
      </div>
      <div className="status-group">
        <Icon name="clock" />
        <span className="status-label">Tasks</span>
        <strong>{enabledTasks} enabled</strong>
      </div>
      <div className="status-group">
        <Icon name={workspaceError || projectAlerts > 0 ? "alert" : "check"} />
        <span className="status-label">Health</span>
        <strong>
          {workspaceError
            ? workspaceError
            : projectAlerts > 0
              ? `${projectAlerts} project alerts`
              : "No active project alerts"}
        </strong>
      </div>
    </footer>
  );
}
