# Quality monitor observations and alert incidents

This module closes **manual evaluation → durable observation → open incident → acknowledgment → observed recovery**. It consumes persisted quality jobs; it does not execute a replacement query, invent counts, or register an automatic scheduler. Automated scheduling and external notification drivers remain separate work.

## Service wiring

`services::quality_alerts::QualityAlertsService::new(resources, jobs, metadata, guard, auth, alerts)` requires all six real dependencies. `guard` is the existing `Arc<QueryAuthorizationService>` with the actual job/metadata repositories and tenant-bound `QueryAccess`. The two drivers are `SqliteQualityAlertRepo::new(SqlitePool)` and `PgQualityAlertRepo::new(PgPool)`. Register the new shared repository module, both driver modules/exports, migration `021_quality_alerts.sql`, context factory field and service module through the root integration only.

| HTTP bridge | Service method | Body / query |
| --- | --- | --- |
| POST `/product/quality-monitors/{id}/evaluate` | `evaluate(actor,id,QualityEvaluateRequest)` | `{expected_version,idempotency_key}` |
| GET `/product/quality-monitors/{id}/observations` | `list_observations(actor,id,limit,offset)` | `limit`, `offset` |
| GET `/product/quality-monitors/{id}/alerts` | `list_alerts(actor,id,status,limit,offset)` | optional `status=open/acknowledged/resolved`, `limit`, `offset` |
| POST `/product/quality-alerts/{id}/acknowledge` | `acknowledge(actor,id,QualityAckRequest)` | `{expected_version,idempotency_key,reason}` |
| GET `/product/quality-alerts/{id}/events` | `list_events(actor,id,limit,offset)` | `limit`, `offset` |
| GET `/product/quality-notices` | `list_notices(actor,unread_only,limit,offset)` | `unread_only`, `limit`, `offset` |
| POST `/product/quality-notices/{id}/read` | `mark_notice_read(actor,id)` | empty body |

The actor is authenticated server-side. Requests reject extra actor, clock and status fields. The evaluation end time is the server's UTC clock, and the half-open window is `[end-window_hours,end)`. A retry uses the same UUID and original expected version. Persisted semantic receipts are read before current resource/CAS checks; replays still require the current active actor and permissions, and released result views still require current data access. A key with different semantics conflicts.

`quality_monitor` retains `object_id`, finite `failure_threshold` in `[0,1]`, and integer `window_hours` in `[1,8760]`. Root may add optional `recipient_ids: string[]` using the real tenant user selector. Each recipient must exist, be active and belong to the same tenant at the write transaction; at most 100 unique recipients are allowed.

## Observations and incident transitions

Every observation freezes the actual monitor, object, task, rule and metadata versions/properties, the real registry fingerprint/columns, run/node identities and results, UTC window and row-rule counting unit. `summary` is derived from actual successful job nodes and independently recounted by the shared repository validator. The transaction compares those proofs against current resources, registry and job/node records before writing.

| Actual evaluation | Observation state | Effect on live incident |
| --- | --- | --- |
| Complete, successful, nonempty checks with ratio **greater than** threshold | `threshold_exceeded` | Create `open`, or retain `open`/`acknowledged` and update the latest observation |
| Complete, successful, nonempty checks within threshold, same policy version | `within_threshold` | Resolve the live incident, recording the actual recovery observation/time |
| Failure, missing result, unfinished run, empty checks or incomplete window | `unavailable` | Retain incident state; update latest observation and append the explicit unavailable reason |
| New threshold version within threshold | `within_threshold` observation | Retain the earlier incident and append `policy_changed`; changing a threshold does not establish recovery |
| Current active operator confirms an open incident | unchanged observation | CAS transition `open → acknowledged`, with operator/time and required handling reason |

An acknowledgment does not claim the data recovered. There is no manual “resolve successfully” endpoint. A subsequent real breach under a new policy rebases the live policy version to that actual observation; later complete recovery can then resolve that policy's incident. A policy-version mismatch remains explicit in the UI.

No actual successful run means `no_results`/`unavailable`, with null ratio/threshold decision. Failed jobs never contribute pretend zero failure counts or a passing threshold decision. The observation includes real completed/failed/incomplete run counts. Sampling is retained as a separate scope; “sample within threshold” does not assert a full-table pass. Ratio/threshold comparison uses the existing quality report's finite floating ratio contract; the underlying integer check counts remain exact. API count fields beyond JavaScript's safe integer range are emitted as decimal strings.

The service scans at most 2,000 tenant quality jobs and freezes at most 512 relevant runs and 256 unique dependencies. An incomplete scan produces `unavailable`, never a complete report. Snapshot size is limited to 512 KiB. If a definition or actual metadata version changed, access/evaluation fails closed and requires a new quality run. The API does not silently omit inaccessible runs to make an aggregate look complete.

## Authorization and transaction boundaries

Observation/incident/event reads require the current active session, `quality:read`, current collector proof and complete-table column grants (or the existing exact system-admin all-data permission). Each successful run's fixed compiler is reconstructed and read through `authorize_quality_result`, with reauthorization/audit before release. Frozen properties, SQL/rule bodies and raw job snapshots are internal and never returned in the public observation view; public dependencies contain only kind/id/version. Mutations additionally require real `quality:write`; the repository rechecks active users and exact role grants in its transaction. Acknowledgment does not use a client-provided user ID.

SQLite uses `BEGIN IMMEDIATE`. PostgreSQL uses a tenant lock and a serializable transaction, sharing locks on actor grants, referenced resources, registry rows and observed jobs/nodes. It verifies the current window's membership too; concurrent completion cannot turn a stale partial scan into an apparently complete observation. Serialization/deadlock conflicts ask the caller to retry with the same key. The generic resource audit uses its valid `update` operation; the domain action is stored in its safe summary, without rule properties, counts, SQL or message content. Immutable observations/events/receipts have append-only database protection and real same-tenant history foreign keys.

## Actual reminders versus message delivery

Opening an incident creates the actual recipient outbox rows in the same transaction, unique by tenant/incident/recipient. Replay cannot duplicate them. Each record carries only the monitor alert name and fixed `/quality/monitoring` link, never checked/failed counts or rule conditions. `delivery_status=pending` is truthful: the existing `MessageDeliveryRepo` driver has not delivered that entry. This implementation does not mark pending entries delivered, claim Email/SMS/webhook success, or fabricate delivery attempts.

The recipient can list and mark their actual reminders read through the authenticated self scope, including when they lack quality read permission. `read_at` is persistent and idempotent. Reading a reminder does not grant access to incident counts. Collector/policy changes still leave the generic reminder visible; protected observation details remain closed until current proof/grants permit access.

`web/src/pages/product/QualityMonitorPage.tsx` owns the definition/observations/incident/self-notice workspace; root wires the existing module route to it. `QualityAlertPanel.tsx` displays actual incident states/events and stable-key acknowledgment. Both use tenant/session/permission-scoped query keys and hide cached counts during fresh authorization checks or errors. Seven targeted UI tests cover failed retries, no fake pass, removed data permission, real status filters, actual self read updates and restricted results. SQLite persistence/service test sources cover atomic transitions/replay, actual recovery from a new run, invalid proofs, cross-tenant recipients, actor permissions, collector availability and DTO boundaries. Cargo validation belongs to the root's serial integration run.
