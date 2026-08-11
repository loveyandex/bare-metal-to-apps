"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { api, type Project } from "@/lib/api";
import { Topbar } from "@/components/paas/topbar";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Boxes, Plus } from "lucide-react";

export default function DashboardPage() {
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listProjects()
      .then(setProjects)
      .catch((e) => setError(String(e.message ?? e)));
  }, []);

  return (
    <>
      <Topbar>
        <Link href="/new">
          <Button size="sm">
            <Plus className="h-4 w-4" />
            New Project
          </Button>
        </Link>
      </Topbar>
      <main className="mx-auto w-full max-w-5xl flex-1 overflow-y-auto px-6 py-10">
        <h1 className="mb-1 text-2xl font-semibold">Projects</h1>
        <p className="mb-8 text-sm text-muted">
          Everything deploys to your local Docker daemon for now — one bridge network per project.
        </p>

        {error && (
          <div className="mb-6 rounded-md border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger">
            Couldn&apos;t reach the API at {process.env.NEXT_PUBLIC_API_BASE ?? "http://localhost:8080"}: {error}
          </div>
        )}

        {projects && projects.length === 0 && (
          <Card className="flex flex-col items-center gap-3 border-dashed py-16 text-center">
            <Boxes className="h-8 w-8 text-muted" />
            <p className="text-sm text-muted">No projects yet.</p>
            <Link href="/new">
              <Button size="sm">
                <Plus className="h-4 w-4" />
                Create your first project
              </Button>
            </Link>
          </Card>
        )}

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {projects?.map((p) => (
            <Link key={p.id} href={`/project/${p.id}`}>
              <Card className="h-full p-5 transition-colors hover:border-primary/60">
                <div className="mb-3 flex h-9 w-9 items-center justify-center rounded-md bg-primary/15 text-primary">
                  <Boxes className="h-4 w-4" />
                </div>
                <h2 className="text-sm font-semibold text-ink">{p.name}</h2>
                <p className="mt-1 text-xs text-muted">{p.slug}</p>
              </Card>
            </Link>
          ))}
        </div>
      </main>
    </>
  );
}
