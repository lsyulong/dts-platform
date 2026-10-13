// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod alert;
mod alert_policy;
mod bi_share;
mod data_access;
mod datasource;
mod master_data;
mod message_delivery;
mod metadata_catalog;
mod monitoring;
mod navigation;
mod notification_connection;
mod organization_members;
mod platform_projection;
mod product_job;
mod product_resource;
mod quality_alerts;
mod role;
mod seed;
mod sensitive_findings;
mod service_access;
mod tag_assignment;
mod task;
mod task_log;
mod task_run;
mod tenant;
mod user;

pub use alert::PgAlertRepo;
pub use alert_policy::PgAlertPolicyRepo;
pub use bi_share::PgBiShareRepo;
pub use data_access::PgDataAccessRepo;
pub use datasource::PgDatasourceRepo;
pub use master_data::PgMasterDataRepo;
pub use message_delivery::PgMessageDeliveryRepo;
pub use metadata_catalog::PgMetadataCatalogRepo;
pub use monitoring::PgMonitoringRepo;
pub use navigation::PgNavigationRepo;
pub use notification_connection::PgNotificationConnectionRepo;
pub use organization_members::PgOrganizationMembersRepo;
pub use platform_projection::PgPlatformProjectionRepo;
pub use product_job::PgProductJobRepo;
pub use product_resource::PgProductResourceRepo;
pub use quality_alerts::PgQualityAlertRepo;
pub use sensitive_findings::PgSensitiveFindingRepo;
pub use service_access::PgServiceAccessRepo;
pub mod development_revisions;
pub mod runtime_configuration;
pub mod standard_governance;
pub use development_revisions::PgDevelopmentRevisionRepo;
pub use role::PgRoleRepo;
pub use runtime_configuration::PgRuntimeConfigurationRepo;
pub use seed::PgSeedRepo;
pub use standard_governance::PgStandardGovernanceRepo;
pub use tag_assignment::PgTagAssignmentRepo;
pub use task::PgTaskRepo;
pub use task_log::PgTaskLogRepo;
pub use task_run::PgTaskRunRepo;
pub use tenant::PgTenantRepo;
pub use user::PgUserRepo;
