"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { api, type DeployMode } from "@/lib/api";
import { Topbar } from "@/components/paas/topbar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { Boxes, Network } from "lucide-react";

const MODES: { value: DeployMode; label: string; description: string; icon: React.ReactNode }[] = [
  {
    value: "docker",
    label: "Docker",
    description: "Services run as containers on your local Docker daemon, one bridge network per project.",
    icon: <Boxes className="h-4 w-4" />,
  },
  {
    value: "kubernetes",
    label: "Kubernetes",
    description: "Services run as Deployments in their own namespace on a cluster reachable via kubectl (e.g. a local kind cluster).",
    icon: <Network className="h-4 w-4" />,
  },
];

export default function NewProjectPage() {
  const router = useRouter();
  const [name, setName] = useState("");
  const [deployMode, setDeployMode] = useState<DeployMode>("docker");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const project = await api.createProject(name.trim(), deployMode);
      router.push(`/project/${project.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSubmitting(false);
    }
  }

  return (
    <>
      <Topbar />
      <main className="mx-auto flex w-full max-w-md flex-1 flex-col justify-center overflow-y-auto px-6 py-10">
        <Card className="p-6">
          <h1 className="mb-1 text-lg font-semibold">New Project</h1>
          <p className="mb-6 text-sm text-muted">
            A project groups services that can talk to each other by name.
          </p>
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="name">Project name</Label>
              <Input
                id="name"
                autoFocus
                placeholder="my-platform"
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label>Deploy target</Label>
              <div className="flex flex-col gap-2">
                {MODES.map((mode) => (
                  <button
                    key={mode.value}
                    type="button"
                    onClick={() => setDeployMode(mode.value)}
                    className={cn(
                      "flex items-start gap-3 rounded-md border px-3 py-2.5 text-left transition-colors",
                      deployMode === mode.value
                        ? "border-primary bg-primary/10"
                        : "border-border bg-canvas hover:bg-surface-2",
                    )}
                  >
                    <div
                      className={cn(
                        "mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md",
                        deployMode === mode.value ? "bg-primary text-primary-foreground" : "bg-surface-2 text-muted",
                      )}
                    >
                      {mode.icon}
                    </div>
                    <div>
                      <p className="text-sm font-medium text-ink">{mode.label}</p>
                      <p className="text-xs text-muted">{mode.description}</p>
                    </div>
                  </button>
                ))}
              </div>
              <p className="text-xs text-muted">
                This can&apos;t be changed after the project is created.
              </p>
            </div>

            {error && <p className="text-xs text-danger">{error}</p>}
            <Button type="submit" disabled={submitting || !name.trim()}>
              {submitting ? "Creating…" : "Create Project"}
            </Button>
          </form>
        </Card>
      </main>
    </>
  );
}
