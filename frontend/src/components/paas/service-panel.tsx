"use client";

import { useEffect, useState, useCallback } from "react";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Tabs, TabsList, TabsTab, TabsPanel } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { StatusBadge } from "@/components/paas/status-badge";
import { VariableValueInput, type LinkSuggestionSource } from "@/components/paas/variable-value-input";
import {
  api,
  type DeployEvent,
  type DeployMode,
  type EnvVarMasked,
  type PortMapping,
  type Service,
  type ServiceDescribe,
} from "@/lib/api";
import { Eye, EyeOff, Plus, RefreshCw, RotateCw, Trash2, X } from "lucide-react";

export function ServicePanel({
  service,
  deployMode,
  linkSuggestions,
  onOpenChange,
  onDeleted,
}: {
  service: Service | null;
  deployMode: DeployMode;
  linkSuggestions: LinkSuggestionSource[];
  onOpenChange: (open: boolean) => void;
  onDeleted: () => void;
}) {
  return (
    <Sheet open={!!service} onOpenChange={onOpenChange}>
      {service && (
        <ServicePanelContent
          key={service.id}
          service={service}
          deployMode={deployMode}
          linkSuggestions={linkSuggestions.filter((s) => s.slug !== service.slug)}
          onDeleted={onDeleted}
        />
      )}
    </Sheet>
  );
}

function ServicePanelContent({
  service,
  deployMode,
  linkSuggestions,
  onDeleted,
}: {
  service: Service;
  deployMode: DeployMode;
  linkSuggestions: LinkSuggestionSource[];
  onDeleted: () => void;
}) {
  const [envVars, setEnvVars] = useState<EnvVarMasked[]>([]);
  const [revealed, setRevealed] = useState<Set<string>>(new Set());
  const [logs, setLogs] = useState<string[]>([]);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [rawMode, setRawMode] = useState(false);
  const [rawText, setRawText] = useState("");
  const [ports, setPortsState] = useState<PortMapping[]>(service.ports);
  const [describe, setDescribe] = useState<ServiceDescribe>(null);
  const [events, setEvents] = useState<DeployEvent[]>([]);
  const [busy, setBusy] = useState(false);

  const loadEnv = useCallback(() => {
    api.listEnv(service.id).then(setEnvVars).catch(() => {});
  }, [service.id]);

  useEffect(() => {
    loadEnv();
  }, [loadEnv]);

  useEffect(() => {
    api.listPorts(service.id).then(setPortsState).catch(() => {});
  }, [service.id]);

  function loadLogs() {
    api.getLogs(service.id).then(setLogs).catch(() => {});
  }

  const loadStatus = useCallback(() => {
    api.describeService(service.id).then(setDescribe).catch(() => {});
    api.listEvents(service.id).then(setEvents).catch(() => {});
  }, [service.id]);

  useEffect(() => {
    loadStatus();
  }, [loadStatus, service.status]);

  async function toggleReveal(v: EnvVarMasked) {
    const next = new Set(revealed);
    if (next.has(v.id)) {
      next.delete(v.id);
      setRevealed(next);
      return;
    }
    if (v.is_secret) {
      const full = await api.listEnv(service.id, true);
      setEnvVars(full);
    }
    next.add(v.id);
    setRevealed(next);
  }

  async function addVar() {
    if (!newKey.trim()) return;
    setBusy(true);
    try {
      await api.setEnv(service.id, [{ key: newKey.trim(), value: newValue }]);
      setNewKey("");
      setNewValue("");
      loadEnv();
    } finally {
      setBusy(false);
    }
  }

  async function applyRaw() {
    if (!rawText.trim()) return;
    setBusy(true);
    try {
      const updated = await api.setEnvRaw(service.id, rawText);
      setEnvVars(updated);
      setRawMode(false);
    } finally {
      setBusy(false);
    }
  }

  async function enterRawMode() {
    // Prefill with the current vars (including real secret values, revealed
    // the same way the eye icon does) so raw mode is an editable snapshot
    // you can append to, not a blank box that silently drops everything not
    // re-typed.
    const full = await api.listEnv(service.id, true);
    setRawText(full.map((v) => `${v.key}=${v.value ?? ""}`).join("\n"));
    setRawMode(true);
  }

  async function removeVar(id: string) {
    await api.deleteEnv(service.id, id);
    loadEnv();
  }

  function addPortRow() {
    setPortsState((prev) => [...prev, { container_port: 3000, host_port: null, protocol: "tcp" }]);
  }

  function updatePort(index: number, patch: Partial<PortMapping>) {
    setPortsState((prev) => prev.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  }

  function removePort(index: number) {
    setPortsState((prev) => prev.filter((_, i) => i !== index));
  }

  async function applyPorts() {
    setBusy(true);
    try {
      await api.setPorts(service.id, ports);
    } finally {
      setBusy(false);
    }
  }

  async function redeploy() {
    setBusy(true);
    try {
      await api.redeployService(service.id);
    } finally {
      setBusy(false);
    }
  }

  async function restart() {
    setBusy(true);
    try {
      await api.restartService(service.id);
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!confirm(`Delete service "${service.name}"? This removes its container permanently.`)) return;
    setBusy(true);
    try {
      await api.deleteService(service.id);
      onDeleted();
    } finally {
      setBusy(false);
    }
  }

  return (
    <SheetContent>
      <SheetHeader>
        <div className="flex items-center gap-2">
          <SheetTitle>{service.name}</SheetTitle>
          <StatusBadge status={service.status} />
        </div>
        <p className="font-mono text-xs text-muted">{service.container_name}</p>
        {service.status_message && (
          <p className="mt-1 text-xs text-muted">{service.status_message}</p>
        )}
      </SheetHeader>

      <div className="mb-4 flex flex-wrap gap-2">
        <Button size="sm" variant="secondary" onClick={restart} disabled={busy} title="Restart the running container/pod in place, without re-applying its spec">
          <RotateCw className="h-3.5 w-3.5" /> Restart
        </Button>
        <Button size="sm" variant="secondary" onClick={redeploy} disabled={busy} title="Re-apply the full spec: image, env vars, ports">
          <RefreshCw className="h-3.5 w-3.5" /> Redeploy
        </Button>
        <Button size="sm" variant="destructive" onClick={remove} disabled={busy}>
          <Trash2 className="h-3.5 w-3.5" /> Delete
        </Button>
      </div>

      <Tabs defaultValue="status">
        <TabsList className="flex-wrap">
          <TabsTab value="status" onClick={loadStatus}>
            Status
          </TabsTab>
          <TabsTab value="variables">Variables</TabsTab>
          <TabsTab value="networking">Networking</TabsTab>
          <TabsTab value="logs" onClick={loadLogs}>
            Logs
          </TabsTab>
          <TabsTab value="events" onClick={loadStatus}>
            Events
          </TabsTab>
        </TabsList>

        <TabsPanel value="status">
          <StatusTabContent describe={describe} status={service.status} statusMessage={service.status_message} onRefresh={loadStatus} />
        </TabsPanel>

        <TabsPanel value="variables">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-xs font-medium text-muted">
              {rawMode ? "Paste a .env block" : "Variables"}
            </p>
            <button
              onClick={() => (rawMode ? setRawMode(false) : enterRawMode())}
              className="text-xs text-primary hover:underline"
            >
              {rawMode ? "Form editor" : "Raw editor"}
            </button>
          </div>

          {rawMode ? (
            <div className="flex flex-col gap-2">
              <textarea
                value={rawText}
                onChange={(e) => setRawText(e.target.value)}
                placeholder={"DATABASE_URL=${{postgres.DATABASE_URL}}\nPORT=3000"}
                rows={8}
                className="w-full rounded-md border border-border bg-canvas px-3 py-2 font-mono text-xs text-ink outline-none focus-visible:ring-2 focus-visible:ring-primary"
              />
              <Button size="sm" onClick={applyRaw} disabled={busy || !rawText.trim()}>
                Apply variables
              </Button>
            </div>
          ) : (
            <>
              <div className="flex flex-col gap-2">
                {envVars.map((v) => (
                  <div
                    key={v.id}
                    className="flex items-center gap-2 rounded-md border border-border bg-canvas px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-mono text-xs font-medium text-ink">{v.key}</p>
                      <p className="truncate font-mono text-xs text-muted">
                        {v.value === null ? "••••••••••••" : v.value || "(empty)"}
                      </p>
                    </div>
                    {v.is_secret && (
                      <button
                        onClick={() => toggleReveal(v)}
                        className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
                        title={revealed.has(v.id) ? "Hide" : "Reveal"}
                      >
                        {revealed.has(v.id) ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                      </button>
                    )}
                    {!v.is_generated && (
                      <button
                        onClick={() => removeVar(v.id)}
                        className="rounded p-1 text-muted hover:bg-surface-2 hover:text-danger"
                      >
                        <X className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </div>
                ))}
                {envVars.length === 0 && <p className="text-xs text-muted">No env vars yet.</p>}
              </div>

              <div className="mt-4 flex flex-col gap-2 border-t border-border pt-4">
                <p className="text-xs font-medium text-muted">
                  Add variable — link to another service with{" "}
                  <code className="rounded bg-surface-2 px-1 py-0.5">{"${{service-slug.KEY}}"}</code>
                </p>
                <div className="flex gap-2">
                  <Input
                    className="flex-1"
                    placeholder="KEY"
                    value={newKey}
                    onChange={(e) => setNewKey(e.target.value)}
                  />
                  <VariableValueInput
                    placeholder="value or ${{postgres.DATABASE_URL}}"
                    value={newValue}
                    onChange={setNewValue}
                    suggestions={linkSuggestions}
                  />
                  <Button size="icon" onClick={addVar} disabled={busy || !newKey.trim()}>
                    <Plus className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </>
          )}
        </TabsPanel>

        <TabsPanel value="networking">
          <p className="mb-3 text-xs text-muted">
            Expose a container port outside the project. Other services on the same{" "}
            {deployMode === "kubernetes" ? "namespace" : "network"} can already reach any port by
            name without publishing it here.
          </p>

          <div className="flex flex-col gap-2">
            {ports.map((p, i) => (
              <div key={i} className="flex items-center gap-2 rounded-md border border-border bg-canvas px-3 py-2">
                <div className="flex flex-1 items-center gap-2">
                  <div className="flex-1">
                    <Label className="mb-1 block normal-case">Container port</Label>
                    <Input
                      type="number"
                      value={p.container_port}
                      onChange={(e) => updatePort(i, { container_port: Number(e.target.value) || 0 })}
                    />
                  </div>
                  <div className="flex-1">
                    <Label className="mb-1 block normal-case">
                      {deployMode === "kubernetes" ? "Forward to localhost port" : "Host port"}
                    </Label>
                    <Input
                      type="number"
                      placeholder="internal only"
                      value={p.host_port ?? ""}
                      onChange={(e) =>
                        updatePort(i, { host_port: e.target.value ? Number(e.target.value) : null })
                      }
                    />
                  </div>
                </div>
                <button
                  onClick={() => removePort(i)}
                  className="mt-4 rounded p-1 text-muted hover:bg-surface-2 hover:text-danger"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
            {ports.length === 0 && <p className="text-xs text-muted">No ports published.</p>}
          </div>

          <div className="mt-3 flex gap-2">
            <Button size="sm" variant="secondary" onClick={addPortRow}>
              <Plus className="h-3.5 w-3.5" /> Add port
            </Button>
            <Button size="sm" onClick={applyPorts} disabled={busy}>
              Apply &amp; redeploy
            </Button>
          </div>

          {deployMode === "kubernetes" && (
            <p className="mt-3 text-xs text-muted">
              The backend runs a <code className="rounded bg-surface-2 px-1 py-0.5">kubectl port-forward</code>{" "}
              for you when you set this, so it&apos;s reachable at{" "}
              <code className="rounded bg-surface-2 px-1 py-0.5">localhost:&lt;port&gt;</code> even on a plain
              local <code className="rounded bg-surface-2 px-1 py-0.5">kind</code> cluster with no{" "}
              <code className="rounded bg-surface-2 px-1 py-0.5">extraPortMappings</code>. The Service also
              gets a matching NodePort if the port is in the 30000-32767 range, for clusters that route
              NodePorts directly.
            </p>
          )}
        </TabsPanel>

        <TabsPanel value="logs">
          <div className="max-h-96 overflow-y-auto rounded-md border border-border bg-canvas p-3 font-mono text-xs text-muted">
            {logs.length === 0 && <p>No logs yet — try redeploying or refreshing.</p>}
            {logs.map((line, i) => (
              <div key={i} className="whitespace-pre-wrap">
                {line}
              </div>
            ))}
          </div>
          <Button size="sm" variant="secondary" className="mt-2" onClick={loadLogs}>
            <RefreshCw className="h-3.5 w-3.5" /> Refresh
          </Button>
        </TabsPanel>

        <TabsPanel value="events">
          <div className="flex flex-col gap-2">
            {events.length === 0 && <p className="text-xs text-muted">No events yet.</p>}
            {events.map((e) => (
              <div key={e.id} className="rounded-md border border-border bg-canvas px-3 py-2">
                <div className="flex items-center gap-2">
                  <StatusBadge status={e.status as Service["status"]} />
                  <span className="text-[10px] text-muted">{new Date(e.created_at).toLocaleTimeString()}</span>
                </div>
                {e.message && <p className="mt-1 text-xs text-muted">{e.message}</p>}
              </div>
            ))}
          </div>
          <Button size="sm" variant="secondary" className="mt-2" onClick={loadStatus}>
            <RefreshCw className="h-3.5 w-3.5" /> Refresh
          </Button>
        </TabsPanel>
      </Tabs>
    </SheetContent>
  );
}

function StatusTabContent({
  describe,
  status,
  statusMessage,
  onRefresh,
}: {
  describe: ServiceDescribe;
  status: Service["status"];
  statusMessage: string | null;
  onRefresh: () => void;
}) {
  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-md border border-border bg-canvas px-3 py-2">
        <div className="flex items-center gap-2">
          <StatusBadge status={status} />
        </div>
        {statusMessage && <p className="mt-1 text-xs text-muted">{statusMessage}</p>}
      </div>

      {describe?.kind === "kubernetes" && (
        <>
          <div>
            <p className="mb-1.5 text-xs font-medium text-muted">Deployment</p>
            {describe.deployment ? (
              <MiniTable
                columns={["Name", "Status", "Pods"]}
                rows={[[
                  describe.deployment.name,
                  describe.deployment.status,
                  `${describe.deployment.ready}/${describe.deployment.replicas}`,
                ]]}
              />
            ) : (
              <p className="text-xs text-muted">No Deployment found yet.</p>
            )}
          </div>
          <div>
            <p className="mb-1.5 text-xs font-medium text-muted">Pods</p>
            {describe.pods.length > 0 ? (
              <MiniTable
                columns={["Name", "Status", "Restarts"]}
                rows={describe.pods.map((p) => [p.name, p.status, String(p.restarts)])}
              />
            ) : (
              <p className="text-xs text-muted">No pods scheduled yet.</p>
            )}
          </div>
        </>
      )}

      {describe?.kind === "docker" && (
        <MiniTable
          columns={["Field", "Value"]}
          rows={[
            ["Image", describe.image ?? "—"],
            ["State", describe.state ?? "—"],
            ["Started at", describe.started_at ?? "—"],
            ["Restart count", String(describe.restart_count ?? 0)],
            ["Exit code", describe.exit_code !== null ? String(describe.exit_code) : "—"],
            ...(describe.error ? [["Error", describe.error]] : []),
          ]}
        />
      )}

      {!describe && <p className="text-xs text-muted">No container/pod deployed yet.</p>}

      <Button size="sm" variant="secondary" onClick={onRefresh} className="self-start">
        <RefreshCw className="h-3.5 w-3.5" /> Refresh
      </Button>
    </div>
  );
}

function MiniTable({ columns, rows }: { columns: string[]; rows: string[][] }) {
  return (
    <div className="overflow-x-auto rounded-md border border-border">
      <table className="w-full text-left text-xs">
        <thead>
          <tr className="border-b border-border bg-canvas text-muted">
            {columns.map((c) => (
              <th key={c} className="px-3 py-1.5 font-medium">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr key={i} className="border-b border-border last:border-0">
              {row.map((cell, j) => (
                <td key={j} className="truncate px-3 py-1.5 font-mono text-ink">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
