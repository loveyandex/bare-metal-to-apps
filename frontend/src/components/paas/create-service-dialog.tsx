"use client";

import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, type DbEngine } from "@/lib/api";
import {
  GitBranch,
  Database,
  Layers,
  Container,
  FunctionSquare,
  Package,
  ChevronRight,
  ArrowLeft,
} from "lucide-react";

type Step = "menu" | "database" | "docker_image";

const DATABASES: { engine: DbEngine; label: string; icon: string }[] = [
  { engine: "postgres", label: "PostgreSQL", icon: "🐘" },
  { engine: "redis", label: "Redis", icon: "📦" },
  { engine: "mongodb", label: "MongoDB", icon: "🍃" },
  { engine: "mysql", label: "MySQL", icon: "🐬" },
];

const IMAGE_EXAMPLES = [
  "hello-world",
  "ghcr.io/username/repo:latest",
  "quay.io/username/repo:tag",
  "registry.gitlab.com/username/repo:tag",
];

export function CreateServiceDialog({
  projectId,
  open,
  onOpenChange,
  onCreated,
}: {
  projectId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}) {
  const [step, setStep] = useState<Step>("menu");
  const [image, setImage] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function reset(open: boolean) {
    onOpenChange(open);
    if (!open) {
      setTimeout(() => {
        setStep("menu");
        setImage("");
        setError(null);
      }, 150);
    }
  }

  async function createFromImage() {
    if (!image.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      await api.createDockerImageService(projectId, image.trim());
      onCreated();
      reset(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  async function createFromDatabase(engine: DbEngine) {
    setSubmitting(true);
    setError(null);
    try {
      await api.createDatabaseService(projectId, engine);
      onCreated();
      reset(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={reset}>
      <DialogContent className="max-w-sm">
        {step === "menu" && (
          <>
            <DialogHeader>
              <DialogTitle>New Service</DialogTitle>
              <DialogDescription>Choose what to deploy into this project.</DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-1">
              <MenuItem icon={<GitBranch className="h-4 w-4" />} label="GitHub Repository" disabled comingSoon />
              <MenuItem
                icon={<Database className="h-4 w-4" />}
                label="Database"
                onClick={() => setStep("database")}
              />
              <MenuItem icon={<Layers className="h-4 w-4" />} label="Template" disabled comingSoon />
              <MenuItem
                icon={<Container className="h-4 w-4" />}
                label="Docker Image"
                onClick={() => setStep("docker_image")}
              />
              <MenuItem icon={<FunctionSquare className="h-4 w-4" />} label="Function" disabled comingSoon />
              <MenuItem icon={<Package className="h-4 w-4" />} label="Bucket" disabled comingSoon />
            </div>
          </>
        )}

        {step === "database" && (
          <>
            <DialogHeader>
              <button
                onClick={() => setStep("menu")}
                className="mb-2 flex items-center gap-1 text-xs text-muted hover:text-ink"
              >
                <ArrowLeft className="h-3 w-3" /> Back
              </button>
              <DialogTitle>Choose a database</DialogTitle>
              <DialogDescription>
                We&apos;ll generate a password and connection env vars automatically.
              </DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-1">
              {DATABASES.map((db) => (
                <button
                  key={db.engine}
                  disabled={submitting}
                  onClick={() => createFromDatabase(db.engine)}
                  className="flex items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm text-ink transition-colors hover:bg-surface-2 disabled:opacity-50"
                >
                  <span className="text-lg leading-none">{db.icon}</span>
                  {db.label}
                </button>
              ))}
            </div>
            {error && <p className="mt-2 text-xs text-danger">{error}</p>}
          </>
        )}

        {step === "docker_image" && (
          <>
            <DialogHeader>
              <button
                onClick={() => setStep("menu")}
                className="mb-2 flex items-center gap-1 text-xs text-muted hover:text-ink"
              >
                <ArrowLeft className="h-3 w-3" /> Back
              </button>
              <DialogTitle>Deploy a Docker image</DialogTitle>
              <DialogDescription>Enter an image from any supported registry.</DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="image">Image</Label>
                <Input
                  id="image"
                  autoFocus
                  placeholder="ghcr.io/love-solana/sqlx:v1.0.13"
                  value={image}
                  onChange={(e) => setImage(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && createFromImage()}
                />
              </div>
              <div className="rounded-md border border-border bg-canvas p-3">
                <p className="mb-2 text-xs font-medium text-muted">Examples</p>
                <ul className="space-y-1 text-xs text-muted">
                  {IMAGE_EXAMPLES.map((ex) => (
                    <li key={ex} className="flex items-center gap-1.5">
                      <span className="h-1 w-1 rounded-full bg-muted" /> {ex}
                    </li>
                  ))}
                </ul>
              </div>
              {error && <p className="text-xs text-danger">{error}</p>}
              <Button onClick={createFromImage} disabled={submitting || !image.trim()}>
                {submitting ? "Deploying…" : "Deploy"}
              </Button>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function MenuItem({
  icon,
  label,
  onClick,
  disabled,
  comingSoon,
}: {
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
  disabled?: boolean;
  comingSoon?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="flex items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm text-ink transition-colors hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-40"
    >
      {icon}
      <span className="flex-1">{label}</span>
      {comingSoon && <span className="text-[10px] text-muted">soon</span>}
      {!disabled && <ChevronRight className="h-4 w-4 text-muted" />}
    </button>
  );
}
