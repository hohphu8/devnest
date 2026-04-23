import { Icon } from "@/components/ui/icons";

interface MetricCardProps {
  label: string;
  value: string;
  tone?: "success" | "warning" | "error";
}

export function MetricCard({ label, value, tone }: MetricCardProps) {
  const toneLabel = tone === "error" ? "Alert" : tone === "warning" ? "Watch" : "Healthy";

  return (
    <section className="metric-card">
      <div className="metric-head">
        <span className="metric-label">{label}</span>
        <span className="status-chip" data-tone={tone}>
          <Icon name={tone === "error" ? "alert" : tone === "warning" ? "activity" : "check"} />
          {toneLabel}
        </span>
      </div>
      <div className="metric-body">
        <span className="metric-value">{value}</span>
        <span className="metric-trace" aria-hidden="true">
          <span />
          <span />
          <span />
          <span />
        </span>
      </div>
    </section>
  );
}
