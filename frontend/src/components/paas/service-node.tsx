"use client";

import { Handle, Position, type NodeProps } from "reactflow";
import { GitBranch, Database as DatabaseIcon, Container } from "lucide-react";
import { StatusBadge } from "@/components/paas/status-badge";
import type { Service } from "@/lib/api";

export type ServiceNodeData = {
  service: Service;
  onOpen: (service: Service) => void;
};

function icon(service: Service) {
  if (service.kind === "database") return <DatabaseIcon className="h-4 w-4" />;
  if (service.deploy_source.type === "docker_image") return <Container className="h-4 w-4" />;
  return <GitBranch className="h-4 w-4" />;
}

function subtitle(service: Service) {
  if (service.deploy_source.type === "database") return service.deploy_source.engine;
  return service.deploy_source.image;
}

export function ServiceNode({ data }: NodeProps<ServiceNodeData>) {
  const { service, onOpen } = data;
  return (
    <button
      onClick={() => onOpen(service)}
      className="w-64 rounded-lg border border-border bg-surface p-4 text-left shadow-lg transition-colors hover:border-primary/60"
    >
      <Handle type="target" position={Position.Left} className="!bg-primary" />
      <div className="flex items-center gap-2">
        <div className="flex h-7 w-7 items-center justify-center rounded-md bg-surface-2 text-ink">
          {icon(service)}
        </div>
        <span className="truncate text-sm font-semibold text-ink">{service.name}</span>
      </div>
      <p className="mt-1 truncate text-xs text-muted">{subtitle(service)}</p>
      <div className="mt-3">
        <StatusBadge status={service.status} />
      </div>
      <Handle type="source" position={Position.Right} className="!bg-primary" />
    </button>
  );
}
