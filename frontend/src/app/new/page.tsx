"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { api } from "@/lib/api";
import { Topbar } from "@/components/paas/topbar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Card } from "@/components/ui/card";

export default function NewProjectPage() {
  const router = useRouter();
  const [name, setName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const project = await api.createProject(name.trim());
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
            A project groups services that talk to each other on their own Docker network.
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
