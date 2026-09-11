ALTER TABLE automations
ADD COLUMN target_provider TEXT
CHECK (target_provider IS NULL OR target_provider IN ('github', 'forgejo'));

ALTER TABLE automations
ADD COLUMN workflow_source_provider TEXT
CHECK (workflow_source_provider IS NULL OR workflow_source_provider IN ('github', 'forgejo'));
