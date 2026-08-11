import { Badge } from "@/components/ui/badge";
import type { DeployMode } from "@/lib/api";
import { Boxes, Network } from "lucide-react";

export function DeployModeBadge({ mode }: { mode: DeployMode }) {
  return (
    <Badge variant="muted">
      {mode === "kubernetes" ? <Network className="h-3 w-3" /> : <Boxes className="h-3 w-3" />}
      {mode === "kubernetes" ? "Kubernetes" : "Docker"}
    </Badge>
  );
}
