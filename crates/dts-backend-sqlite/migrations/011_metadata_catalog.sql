CREATE TABLE IF NOT EXISTS metadata_catalog (
    tenant_id TEXT NOT NULL,
    engine TEXT NOT NULL CHECK (engine = 'swanlake'),
    table_catalog TEXT NOT NULL,
    schema_name TEXT NOT NULL,
    table_name TEXT NOT NULL,
    object_kind TEXT NOT NULL DEFAULT 'metadata_object' CHECK (object_kind = 'metadata_object'),
    object_id TEXT NOT NULL,
    object_version INTEGER NOT NULL CHECK (object_version > 0),
    table_type TEXT NOT NULL CHECK (table_type IN ('BASE TABLE', 'VIEW')),
    columns_snapshot TEXT NOT NULL CHECK (json_valid(columns_snapshot) AND json_type(columns_snapshot) = 'array'),
    source_fingerprint TEXT NOT NULL CHECK (length(source_fingerprint) = 64),
    discovery_status TEXT NOT NULL CHECK (discovery_status IN ('present', 'missing')),
    last_collection_id TEXT NOT NULL,
    last_observed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, engine, table_catalog, schema_name, table_name),
    UNIQUE (tenant_id, object_id),
    FOREIGN KEY (tenant_id, object_kind, object_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_metadata_catalog_object ON metadata_catalog(tenant_id, object_id, object_version);
