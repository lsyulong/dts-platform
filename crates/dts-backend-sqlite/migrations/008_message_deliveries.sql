CREATE UNIQUE INDEX IF NOT EXISTS idx_user_tenant_id_delivery
    ON "user"(tenant_id, id);

CREATE TABLE IF NOT EXISTS message_delivery_batch (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    source_kind TEXT NOT NULL DEFAULT 'system_message' CHECK (source_kind = 'system_message'),
    message_id TEXT NOT NULL,
    message_version INTEGER NOT NULL CHECK (message_version > 0),
    resource_version INTEGER NOT NULL CHECK (resource_version = message_version + 1),
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request TEXT NOT NULL CHECK (json_valid(request)),
    delivered_at TEXT NOT NULL,
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, id, message_id, message_version),
    UNIQUE (tenant_id, idempotency_key),
    UNIQUE (tenant_id, message_id, message_version),
    FOREIGN KEY (tenant_id, source_kind, message_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id)
        REFERENCES "user"(tenant_id, id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS message_delivery (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    source_kind TEXT NOT NULL DEFAULT 'system_message' CHECK (source_kind = 'system_message'),
    message_id TEXT NOT NULL,
    message_version INTEGER NOT NULL CHECK (message_version > 0),
    recipient_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    delivered_at TEXT NOT NULL,
    read_at TEXT,
    UNIQUE (tenant_id, message_id, message_version, recipient_id),
    FOREIGN KEY (tenant_id, batch_id, message_id, message_version)
        REFERENCES message_delivery_batch(tenant_id, id, message_id, message_version) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, source_kind, message_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, recipient_id)
        REFERENCES "user"(tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_message_delivery_inbox
    ON message_delivery(tenant_id, recipient_id, delivered_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_message_delivery_batch
    ON message_delivery(tenant_id, batch_id);
