alter table projects add column deploy_mode text not null default 'docker'; -- 'docker' | 'kubernetes'
alter table services add column ports jsonb not null default '[]'::jsonb; -- [{container_port, host_port, protocol}]
