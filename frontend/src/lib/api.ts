export const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? "http://localhost:8080";
export const WS_BASE = API_BASE.replace(/^http/, "ws");

export type ServiceKind = "app" | "database";
export type DbEngine = "postgres" | "redis" | "mysql" | "mongodb";

export type DeploySource =
  | { type: "docker_image"; image: string }
  | { type: "database"; engine: DbEngine };

export type ServiceStatus = "creating" | "running" | "crashed" | "failed" | "stopped" | "deleting";

export interface Project {
  id: string;
  name: string;
  slug: string;
  docker_network: string;
  created_at: string;
}

export interface Service {
  id: string;
  project_id: string;
  name: string;
  slug: string;
  kind: ServiceKind;
  deploy_source: DeploySource;
  status: ServiceStatus;
  status_message: string | null;
  container_id: string | null;
  container_name: string;
  desired_replicas: number;
  created_at: string;
  updated_at: string;
}

export interface ProjectDetail extends Project {
  services: Service[];
}

export interface EnvVarMasked {
  id: string;
  key: string;
  value: string | null;
  is_secret: boolean;
  is_generated: boolean;
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: { "Content-Type": "application/json", ...init?.headers },
    cache: "no-store",
  });
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    throw new Error(text || `request failed: ${res.status}`);
  }
  if (res.status === 204 || res.status === 202) return undefined as T;
  return res.json() as Promise<T>;
}

export const api = {
  listProjects: () => req<Project[]>("/api/projects"),
  createProject: (name: string) =>
    req<Project>("/api/projects", { method: "POST", body: JSON.stringify({ name }) }),
  getProject: (id: string) => req<ProjectDetail>(`/api/projects/${id}`),
  deleteProject: (id: string) => req<void>(`/api/projects/${id}`, { method: "DELETE" }),

  createDockerImageService: (projectId: string, image: string, name?: string) =>
    req<Service>(`/api/projects/${projectId}/services`, {
      method: "POST",
      body: JSON.stringify({ source: "docker_image", image, name: name || null }),
    }),
  createDatabaseService: (projectId: string, engine: DbEngine, name?: string) =>
    req<Service>(`/api/projects/${projectId}/services`, {
      method: "POST",
      body: JSON.stringify({ source: "database", engine, name: name || null }),
    }),
  getService: (id: string) => req<Service>(`/api/services/${id}`),
  deleteService: (id: string) => req<void>(`/api/services/${id}`, { method: "DELETE" }),
  redeployService: (id: string) => req<void>(`/api/services/${id}/redeploy`, { method: "POST" }),
  getLogs: (id: string) => req<string[]>(`/api/services/${id}/logs`),

  listEnv: (id: string, reveal = false) =>
    req<EnvVarMasked[]>(`/api/services/${id}/env${reveal ? "?reveal=true" : ""}`),
  setEnv: (id: string, vars: { key: string; value: string; is_secret?: boolean }[]) =>
    req<void>(`/api/services/${id}/env`, { method: "PUT", body: JSON.stringify({ vars }) }),
  deleteEnv: (serviceId: string, envId: string) =>
    req<void>(`/api/services/${serviceId}/env/${envId}`, { method: "DELETE" }),
};

export type WsEvent =
  | { type: "service_status"; service_id: string; project_id: string; status: ServiceStatus; message: string | null }
  | { type: "service_log"; service_id: string; line: string }
  | { type: "service_created"; project_id: string; service_id: string }
  | { type: "service_deleted"; project_id: string; service_id: string };
