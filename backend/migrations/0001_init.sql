create extension if not exists "uuid-ossp";

create table projects (
    id uuid primary key default uuid_generate_v4(),
    name text not null,
    slug text not null unique,
    docker_network text not null,
    created_at timestamptz not null default now()
);

create table services (
    id uuid primary key default uuid_generate_v4(),
    project_id uuid not null references projects(id) on delete cascade,
    name text not null,
    slug text not null,
    kind text not null,               -- 'app' | 'database'
    deploy_source jsonb not null,      -- { type: "docker_image", image } | { type: "database", engine }
    status text not null default 'creating', -- creating|running|crashed|failed|stopped|deleting
    status_message text,
    container_id text,
    container_name text not null,
    desired_replicas int not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (project_id, slug)
);

create table env_vars (
    id uuid primary key default uuid_generate_v4(),
    service_id uuid not null references services(id) on delete cascade,
    key text not null,
    value text not null,
    is_secret boolean not null default false,
    is_generated boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (service_id, key)
);

create table deploy_events (
    id uuid primary key default uuid_generate_v4(),
    service_id uuid not null references services(id) on delete cascade,
    status text not null,
    message text,
    created_at timestamptz not null default now()
);
