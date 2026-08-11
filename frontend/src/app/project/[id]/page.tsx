"use client";

import { use, useCallback, useEffect, useMemo, useState } from "react";
import ReactFlow, { Background, Controls, type Edge, type Node } from "reactflow";
import "reactflow/dist/style.css";
import { api, type EnvVarMasked, type ProjectDetail, type Service } from "@/lib/api";
import { useLiveEvents } from "@/lib/use-ws";
import { Topbar } from "@/components/paas/topbar";
import { ServiceNode } from "@/components/paas/service-node";
import { CreateServiceDialog } from "@/components/paas/create-service-dialog";
import { ServicePanel } from "@/components/paas/service-panel";
import { Button } from "@/components/ui/button";
import { Plus } from "lucide-react";

const nodeTypes = { service: ServiceNode };

export default function ProjectPage({ params }: PageProps<"/project/[id]">) {
  const { id } = use(params);
  const [project, setProject] = useState<ProjectDetail | null>(null);
  const [envByService, setEnvByService] = useState<Record<string, EnvVarMasked[]>>({});
  const [createOpen, setCreateOpen] = useState(false);
  const [activeServiceId, setActiveServiceId] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api
      .getProject(id)
      .then(async (p) => {
        setProject(p);
        const pairs = await Promise.all(
          p.services.map(async (s) => [s.id, await api.listEnv(s.id).catch(() => [])] as const),
        );
        setEnvByService(Object.fromEntries(pairs));
      })
      .catch(() => {});
  }, [id]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useLiveEvents(
    useCallback(
      (event) => {
        if ("project_id" in event && event.project_id !== id) return;
        if (event.type === "service_status") {
          setProject((prev) =>
            prev
              ? {
                  ...prev,
                  services: prev.services.map((s) =>
                    s.id === event.service_id
                      ? { ...s, status: event.status, status_message: event.message }
                      : s,
                  ),
                }
              : prev,
          );
        } else if (event.type === "service_created" || event.type === "service_deleted") {
          refresh();
        }
      },
      [id, refresh],
    ),
  );

  const activeService = useMemo(
    () => project?.services.find((s) => s.id === activeServiceId) ?? null,
    [project, activeServiceId],
  );

  const nodes: Node[] = useMemo(
    () =>
      (project?.services ?? []).map((service, i) => ({
        id: service.id,
        type: "service",
        position: { x: (i % 3) * 300, y: Math.floor(i / 3) * 180 },
        data: { service, onOpen: (s: Service) => setActiveServiceId(s.id) },
      })),
    [project],
  );

  const edges: Edge[] = useMemo(
    () => linkEdges(project?.services ?? [], envByService),
    [project, envByService],
  );

  if (!project) {
    return (
      <>
        <Topbar />
        <main className="flex flex-1 items-center justify-center text-sm text-muted">Loading…</main>
      </>
    );
  }

  return (
    <>
      <Topbar>
        <span className="text-sm text-muted">{project.name}</span>
      </Topbar>
      <div className="relative flex-1">
        <div className="absolute right-4 top-4 z-10 flex gap-2">
          <Button size="sm" variant="secondary" onClick={refresh}>
            Sync
          </Button>
          <Button size="sm" onClick={() => setCreateOpen(true)}>
            <Plus className="h-4 w-4" /> Create
          </Button>
        </div>

        {project.services.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 bg-dot-grid">
            <p className="text-sm text-muted">This project has no services yet.</p>
            <Button onClick={() => setCreateOpen(true)}>
              <Plus className="h-4 w-4" /> Create your first service
            </Button>
          </div>
        ) : (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            fitView
            className="bg-canvas bg-dot-grid"
            proOptions={{ hideAttribution: true }}
          >
            <Background color="transparent" />
            <Controls className="!bg-surface !border-border [&_button]:!bg-surface [&_button]:!border-border [&_button]:!fill-white" />
          </ReactFlow>
        )}
      </div>

      <CreateServiceDialog
        projectId={id}
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={refresh}
      />
      <ServicePanel
        service={activeService}
        onOpenChange={(open) => !open && setActiveServiceId(null)}
        onDeleted={() => {
          setActiveServiceId(null);
          refresh();
        }}
      />
    </>
  );
}

/** Draws an edge between a service and any other service its env vars reference via ${{slug.KEY}}. */
function linkEdges(services: Service[], envByService: Record<string, EnvVarMasked[]>): Edge[] {
  const bySlug = new Map(services.map((s) => [s.slug, s]));
  const edges: Edge[] = [];
  const seen = new Set<string>();
  const pattern = /\$\{\{\s*([a-zA-Z0-9-]+)\.[a-zA-Z0-9_]+\s*\}\}/g;

  for (const service of services) {
    const vars = envByService[service.id] ?? [];
    for (const v of vars) {
      if (!v.value) continue;
      for (const match of v.value.matchAll(pattern)) {
        const targetSlug = match[1];
        const target = bySlug.get(targetSlug);
        if (!target || target.id === service.id) continue;
        const key = `${target.id}->${service.id}`;
        if (seen.has(key)) continue;
        seen.add(key);
        edges.push({
          id: key,
          source: target.id,
          target: service.id,
          animated: true,
          style: { stroke: "var(--color-primary)" },
        });
      }
    }
  }
  return edges;
}
