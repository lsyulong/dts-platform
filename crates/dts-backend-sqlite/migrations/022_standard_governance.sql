CREATE UNIQUE INDEX IF NOT EXISTS idx_standard_actor_scope ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS standard_revision (
 tenant_id TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN('naming_standard','field_standard','enum_standard','unit_standard')),id TEXT NOT NULL,
 revision_version INTEGER NOT NULL CHECK(revision_version>0),definition_hash TEXT NOT NULL CHECK(length(definition_hash)=64),
 payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB))<=524288),published_by TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN('active','withdrawn')),state_version INTEGER NOT NULL CHECK(state_version>0),
 withdrawn_by TEXT,withdrawn_at TEXT,withdrawal_reason TEXT,
 PRIMARY KEY(tenant_id,kind,id,revision_version),FOREIGN KEY(tenant_id,kind,id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,withdrawn_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,published_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
 CHECK((state='active' AND withdrawn_by IS NULL AND withdrawn_at IS NULL AND withdrawal_reason IS NULL) OR (state='withdrawn' AND withdrawn_by IS NOT NULL AND withdrawn_at IS NOT NULL AND withdrawal_reason IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS standard_revision_child (
 tenant_id TEXT NOT NULL,parent_kind TEXT NOT NULL,parent_id TEXT NOT NULL,parent_version INTEGER NOT NULL,position INTEGER NOT NULL CHECK(position>=0),
 child_kind TEXT NOT NULL,child_id TEXT NOT NULL,child_version INTEGER NOT NULL,
 PRIMARY KEY(tenant_id,parent_kind,parent_id,parent_version,position),UNIQUE(tenant_id,parent_kind,parent_id,parent_version,child_kind,child_id),
 FOREIGN KEY(tenant_id,parent_kind,parent_id,parent_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,child_kind,child_id,child_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_binding (
 tenant_id TEXT NOT NULL,model_kind TEXT NOT NULL DEFAULT 'data_model' CHECK(model_kind='data_model'),model_id TEXT NOT NULL,binding_version INTEGER NOT NULL CHECK(binding_version>0),
 payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB))<=262144),created_by TEXT NOT NULL,
 PRIMARY KEY(tenant_id,model_id,binding_version),FOREIGN KEY(tenant_id,model_kind,model_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,created_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_current (
 tenant_id TEXT NOT NULL,model_id TEXT NOT NULL,binding_version INTEGER NOT NULL,
 PRIMARY KEY(tenant_id,model_id),FOREIGN KEY(tenant_id,model_id,binding_version) REFERENCES standard_model_binding(tenant_id,model_id,binding_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_binding_revision (
 tenant_id TEXT NOT NULL,model_id TEXT NOT NULL,binding_version INTEGER NOT NULL,kind TEXT NOT NULL,id TEXT NOT NULL,revision_version INTEGER NOT NULL,
 PRIMARY KEY(tenant_id,model_id,binding_version,kind,id,revision_version),
 FOREIGN KEY(tenant_id,model_id,binding_version) REFERENCES standard_model_binding(tenant_id,model_id,binding_version) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,kind,id,revision_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_gate (
 gate_id TEXT NOT NULL,tenant_id TEXT NOT NULL,actor_id TEXT NOT NULL,model_kind TEXT NOT NULL DEFAULT 'data_model' CHECK(model_kind='data_model'),model_id TEXT NOT NULL,
 claim_version INTEGER NOT NULL CHECK(claim_version>0),payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB))<=1048576),
 phase TEXT NOT NULL CHECK(phase IN('claimed','superseded','finished')),outcome_hash TEXT,result TEXT CHECK(result IS NULL OR json_valid(result)),
 PRIMARY KEY(tenant_id,gate_id),FOREIGN KEY(tenant_id,model_kind,model_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
 CHECK((phase IN('claimed','superseded') AND outcome_hash IS NULL AND result IS NULL) OR (phase='finished' AND length(outcome_hash)=64 AND result IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS standard_model_gate_pending ON standard_model_gate(tenant_id,model_id) WHERE phase='claimed';
CREATE TABLE IF NOT EXISTS standard_model_gate_revision (
 tenant_id TEXT NOT NULL,gate_id TEXT NOT NULL,kind TEXT NOT NULL,id TEXT NOT NULL,revision_version INTEGER NOT NULL,
 PRIMARY KEY(tenant_id,gate_id,kind,id,revision_version),
 FOREIGN KEY(tenant_id,gate_id) REFERENCES standard_model_gate(tenant_id,gate_id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,kind,id,revision_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_inspection (
 tenant_id TEXT NOT NULL,id TEXT NOT NULL,model_id TEXT NOT NULL,binding_version INTEGER NOT NULL,metadata_kind TEXT NOT NULL DEFAULT 'metadata_object' CHECK(metadata_kind='metadata_object'),metadata_id TEXT NOT NULL,
 actor_id TEXT NOT NULL,snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB))<=524288),
 PRIMARY KEY(tenant_id,id),FOREIGN KEY(tenant_id,model_id,binding_version) REFERENCES standard_model_binding(tenant_id,model_id,binding_version) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,metadata_kind,metadata_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TRIGGER IF NOT EXISTS standard_revision_immutable BEFORE UPDATE ON standard_revision
 WHEN NEW.tenant_id<>OLD.tenant_id OR NEW.kind<>OLD.kind OR NEW.id<>OLD.id OR NEW.revision_version<>OLD.revision_version OR NEW.definition_hash<>OLD.definition_hash OR NEW.payload<>OLD.payload OR NEW.published_by<>OLD.published_by OR OLD.state='withdrawn' OR NEW.state<>'withdrawn' OR NEW.state_version<>OLD.state_version+1
 BEGIN SELECT RAISE(ABORT,'immutable standard revision definition or invalid lifecycle'); END;
CREATE TRIGGER IF NOT EXISTS standard_revision_no_delete BEFORE DELETE ON standard_revision BEGIN SELECT RAISE(ABORT,'immutable standard revision'); END;
CREATE TRIGGER IF NOT EXISTS standard_binding_no_update BEFORE UPDATE ON standard_model_binding BEGIN SELECT RAISE(ABORT,'immutable standard model binding'); END;
CREATE TRIGGER IF NOT EXISTS standard_binding_no_delete BEFORE DELETE ON standard_model_binding BEGIN SELECT RAISE(ABORT,'immutable standard model binding'); END;
CREATE TRIGGER IF NOT EXISTS standard_gate_protection BEFORE UPDATE ON standard_model_gate
 WHEN NEW.gate_id<>OLD.gate_id OR NEW.tenant_id<>OLD.tenant_id OR NEW.actor_id<>OLD.actor_id OR NEW.model_kind<>OLD.model_kind OR NEW.model_id<>OLD.model_id OR NEW.claim_version<>OLD.claim_version OR NEW.payload<>OLD.payload OR NOT ((OLD.phase='claimed' AND NEW.phase IN('superseded','finished')) OR (OLD.phase='superseded' AND NEW.phase='finished'))
 BEGIN SELECT RAISE(ABORT,'immutable standard gate or terminal outcome'); END;
CREATE TRIGGER IF NOT EXISTS standard_gate_no_delete BEFORE DELETE ON standard_model_gate BEGIN SELECT RAISE(ABORT,'immutable standard gate'); END;
CREATE TRIGGER IF NOT EXISTS standard_inspection_no_update BEFORE UPDATE ON standard_inspection BEGIN SELECT RAISE(ABORT,'immutable standard inspection'); END;
CREATE TRIGGER IF NOT EXISTS standard_inspection_no_delete BEFORE DELETE ON standard_inspection BEGIN SELECT RAISE(ABORT,'immutable standard inspection'); END;
