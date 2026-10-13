CREATE TABLE IF NOT EXISTS asset_publication (
    tenant_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_kind TEXT NOT NULL DEFAULT 'asset_entry' CHECK (asset_kind='asset_entry'),
    asset_version INTEGER NOT NULL CHECK (asset_version>0),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('metadata_object','data_model','atomic_metric','derived_metric','composite_metric','field_standard','enum_standard','unit_standard')),
    source_id TEXT NOT NULL,
    source_version INTEGER NOT NULL CHECK (source_version>0),
    definition_hash TEXT NOT NULL CHECK (length(definition_hash)=64),
    source_snapshot TEXT NOT NULL CHECK (json_valid(source_snapshot) AND json_type(source_snapshot)='object'),
    proof TEXT NOT NULL CHECK (json_valid(proof) AND json_type(proof)='object'),
    published_by TEXT NOT NULL,
    published_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id,asset_id,asset_version),
    FOREIGN KEY (tenant_id,asset_kind,asset_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,source_kind,source_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,published_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TRIGGER IF NOT EXISTS asset_publication_no_update BEFORE UPDATE ON asset_publication
BEGIN SELECT RAISE(ABORT,'asset_publication_immutable'); END;
CREATE TRIGGER IF NOT EXISTS asset_publication_no_delete BEFORE DELETE ON asset_publication
BEGIN SELECT RAISE(ABORT,'asset_publication_immutable'); END;
CREATE INDEX IF NOT EXISTS idx_asset_audit_period ON data_access_audit(tenant_id,action,created_at,id);
