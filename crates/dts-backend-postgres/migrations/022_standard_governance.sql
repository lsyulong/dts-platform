CREATE UNIQUE INDEX IF NOT EXISTS idx_standard_actor_scope ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS standard_revision (
 tenant_id TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN('naming_standard','field_standard','enum_standard','unit_standard')),id TEXT NOT NULL,
 revision_version BIGINT NOT NULL CHECK(revision_version>0),definition_hash TEXT NOT NULL CHECK(length(definition_hash)=64),
 payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object' AND octet_length(payload)<=524288),published_by TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN('active','withdrawn')),state_version BIGINT NOT NULL CHECK(state_version>0),
 withdrawn_by TEXT,withdrawn_at TIMESTAMPTZ,withdrawal_reason TEXT,
 PRIMARY KEY(tenant_id,kind,id,revision_version),FOREIGN KEY(tenant_id,kind,id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,withdrawn_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,published_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
 CHECK((state='active' AND withdrawn_by IS NULL AND withdrawn_at IS NULL AND withdrawal_reason IS NULL) OR (state='withdrawn' AND withdrawn_by IS NOT NULL AND withdrawn_at IS NOT NULL AND withdrawal_reason IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS standard_revision_child (
 tenant_id TEXT NOT NULL,parent_kind TEXT NOT NULL,parent_id TEXT NOT NULL,parent_version BIGINT NOT NULL,position BIGINT NOT NULL CHECK(position>=0),
 child_kind TEXT NOT NULL,child_id TEXT NOT NULL,child_version BIGINT NOT NULL,
 PRIMARY KEY(tenant_id,parent_kind,parent_id,parent_version,position),UNIQUE(tenant_id,parent_kind,parent_id,parent_version,child_kind,child_id),
 FOREIGN KEY(tenant_id,parent_kind,parent_id,parent_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,child_kind,child_id,child_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_binding (
 tenant_id TEXT NOT NULL,model_kind TEXT NOT NULL DEFAULT 'data_model' CHECK(model_kind='data_model'),model_id TEXT NOT NULL,binding_version BIGINT NOT NULL CHECK(binding_version>0),
 payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object' AND octet_length(payload)<=262144),created_by TEXT NOT NULL,
 PRIMARY KEY(tenant_id,model_id,binding_version),FOREIGN KEY(tenant_id,model_kind,model_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,created_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_current (
 tenant_id TEXT NOT NULL,model_id TEXT NOT NULL,binding_version BIGINT NOT NULL,
 PRIMARY KEY(tenant_id,model_id),FOREIGN KEY(tenant_id,model_id,binding_version) REFERENCES standard_model_binding(tenant_id,model_id,binding_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_binding_revision (
 tenant_id TEXT NOT NULL,model_id TEXT NOT NULL,binding_version BIGINT NOT NULL,kind TEXT NOT NULL,id TEXT NOT NULL,revision_version BIGINT NOT NULL,
 PRIMARY KEY(tenant_id,model_id,binding_version,kind,id,revision_version),
 FOREIGN KEY(tenant_id,model_id,binding_version) REFERENCES standard_model_binding(tenant_id,model_id,binding_version) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,kind,id,revision_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_model_gate (
 gate_id TEXT NOT NULL,tenant_id TEXT NOT NULL,actor_id TEXT NOT NULL,model_kind TEXT NOT NULL DEFAULT 'data_model' CHECK(model_kind='data_model'),model_id TEXT NOT NULL,
 claim_version BIGINT NOT NULL CHECK(claim_version>0),payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object' AND octet_length(payload)<=1048576),
 phase TEXT NOT NULL CHECK(phase IN('claimed','superseded','finished')),outcome_hash TEXT,result TEXT CHECK(result IS NULL OR jsonb_typeof(result::jsonb)='object'),
 PRIMARY KEY(tenant_id,gate_id),FOREIGN KEY(tenant_id,model_kind,model_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
 CHECK((phase IN('claimed','superseded') AND outcome_hash IS NULL AND result IS NULL) OR (phase='finished' AND length(outcome_hash)=64 AND result IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS standard_model_gate_pending ON standard_model_gate(tenant_id,model_id) WHERE phase='claimed';
CREATE TABLE IF NOT EXISTS standard_model_gate_revision (
 tenant_id TEXT NOT NULL,gate_id TEXT NOT NULL,kind TEXT NOT NULL,id TEXT NOT NULL,revision_version BIGINT NOT NULL,
 PRIMARY KEY(tenant_id,gate_id,kind,id,revision_version),
 FOREIGN KEY(tenant_id,gate_id) REFERENCES standard_model_gate(tenant_id,gate_id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,kind,id,revision_version) REFERENCES standard_revision(tenant_id,kind,id,revision_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS standard_inspection (
 tenant_id TEXT NOT NULL,id TEXT NOT NULL,model_id TEXT NOT NULL,binding_version BIGINT NOT NULL,metadata_kind TEXT NOT NULL DEFAULT 'metadata_object' CHECK(metadata_kind='metadata_object'),metadata_id TEXT NOT NULL,
 actor_id TEXT NOT NULL,snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object' AND octet_length(payload)<=524288),
 PRIMARY KEY(tenant_id,id),FOREIGN KEY(tenant_id,model_id,binding_version) REFERENCES standard_model_binding(tenant_id,model_id,binding_version) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,metadata_kind,metadata_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION standard_evidence_immutable() RETURNS trigger AS $$ BEGIN
 RAISE EXCEPTION 'immutable standard governance evidence' USING ERRCODE='23514';
END; $$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION standard_revision_lifecycle() RETURNS trigger AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'immutable standard revision' USING ERRCODE='23514'; END IF;
 IF ROW(NEW.tenant_id,NEW.kind,NEW.id,NEW.revision_version,NEW.definition_hash,NEW.payload,NEW.published_by) IS DISTINCT FROM ROW(OLD.tenant_id,OLD.kind,OLD.id,OLD.revision_version,OLD.definition_hash,OLD.payload,OLD.published_by) OR OLD.state='withdrawn' OR NEW.state<>'withdrawn' OR NEW.state_version<>OLD.state_version+1 THEN
  RAISE EXCEPTION 'immutable standard revision or invalid lifecycle' USING ERRCODE='23514';
 END IF; RETURN NEW;
END; $$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION standard_gate_lifecycle() RETURNS trigger AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'immutable standard gate' USING ERRCODE='23514'; END IF;
 IF ROW(NEW.gate_id,NEW.tenant_id,NEW.actor_id,NEW.model_kind,NEW.model_id,NEW.claim_version,NEW.payload) IS DISTINCT FROM ROW(OLD.gate_id,OLD.tenant_id,OLD.actor_id,OLD.model_kind,OLD.model_id,OLD.claim_version,OLD.payload) OR NOT ((OLD.phase='claimed' AND NEW.phase IN('superseded','finished')) OR (OLD.phase='superseded' AND NEW.phase='finished')) THEN
  RAISE EXCEPTION 'immutable standard gate or terminal outcome' USING ERRCODE='23514';
 END IF; RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS standard_revision_protection ON standard_revision;
CREATE TRIGGER standard_revision_protection BEFORE UPDATE OR DELETE ON standard_revision FOR EACH ROW EXECUTE FUNCTION standard_revision_lifecycle();
DROP TRIGGER IF EXISTS standard_binding_protection ON standard_model_binding;
CREATE TRIGGER standard_binding_protection BEFORE UPDATE OR DELETE ON standard_model_binding FOR EACH ROW EXECUTE FUNCTION standard_evidence_immutable();
DROP TRIGGER IF EXISTS standard_gate_protection ON standard_model_gate;
CREATE TRIGGER standard_gate_protection BEFORE UPDATE OR DELETE ON standard_model_gate FOR EACH ROW EXECUTE FUNCTION standard_gate_lifecycle();
DROP TRIGGER IF EXISTS standard_inspection_protection ON standard_inspection;
CREATE TRIGGER standard_inspection_protection BEFORE UPDATE OR DELETE ON standard_inspection FOR EACH ROW EXECUTE FUNCTION standard_evidence_immutable();

