import { Badge } from "@/components/ui/badge";
import type { ServiceStatus } from "@/lib/api";
import { Loader2 } from "lucide-react";

const STYLES: Record<ServiceStatus, { variant: "default" | "success" | "warning" | "danger" | "muted"; label: string }> = {
  creating: { variant: "warning", label: "Deploying" },
  running: { variant: "success", label: "Running" },
  crashed: { variant: "danger", label: "Crashed" },
  failed: { variant: "danger", label: "Failed" },
  stopped: { variant: "muted", label: "Stopped" },
  deleting: { variant: "muted", label: "Deleting" },
};

export function StatusBadge({ status }: { status: ServiceStatus }) {
  const style = STYLES[status] ?? STYLES.stopped;
  return (
    <Badge variant={style.variant}>
      {status === "creating" ? (
        <Loader2 className="h-3 w-3 animate-spin" />
      ) : (
        <span
          className={`h-1.5 w-1.5 rounded-full ${
            status === "running"
              ? "bg-success"
              : status === "crashed" || status === "failed"
                ? "bg-danger"
                : "bg-muted"
          }`}
        />
      )}
      {style.label}
    </Badge>
  );
}
