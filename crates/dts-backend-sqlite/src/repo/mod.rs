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

pub mod alert;
pub mod alert_policy;
pub mod bi_share;
pub mod data_access;
pub mod datasource;
pub mod master_data;
pub mod message_delivery;
pub mod metadata_catalog;
pub mod monitoring;
pub mod navigation;
pub mod notification_connection;
pub mod organization_members;
pub mod platform_projection;
pub mod product_job;
pub mod product_resource;
pub mod quality_alerts;
pub mod role;
pub mod seed;
pub mod sensitive_findings;
pub mod service_access;
pub mod task;
pub mod task_log;
pub mod task_run;
pub mod tenant;
pub mod user;
mod util;

pub use alert::SqliteAlertRepo;
pub use alert_policy::SqliteAlertPolicyRepo;
pub use bi_share::SqliteBiShareRepo;
pub use data_access::SqliteDataAccessRepo;
pub use datasource::SqliteDatasourceRepo;
pub use master_data::SqliteMasterDataRepo;
pub use message_delivery::SqliteMessageDeliveryRepo;
pub use metadata_catalog::SqliteMetadataCatalogRepo;
pub use monitoring::SqliteMonitoringRepo;
pub use navigation::SqliteNavigationRepo;
pub use notification_connection::SqliteNotificationConnectionRepo;
pub use organization_members::SqliteOrganizationMembersRepo;
pub use platform_projection::SqlitePlatformProjectionRepo;
pub use product_job::SqliteProductJobRepo;
pub use product_resource::SqliteProductResourceRepo;
pub use quality_alerts::SqliteQualityAlertRepo;
pub use sensitive_findings::SqliteSensitiveFindingRepo;
pub use service_access::SqliteServiceAccessRepo;
pub mod development_revisions;
pub mod runtime_configuration;
pub mod standard_governance;
pub mod tag_assignment;
pub use development_revisions::SqliteDevelopmentRevisionRepo;
pub use role::SqliteRoleRepo;
pub use runtime_configuration::SqliteRuntimeConfigurationRepo;
pub use seed::SqliteSeedRepo;
pub use standard_governance::SqliteStandardGovernanceRepo;
pub use tag_assignment::SqliteTagAssignmentRepo;
pub use task::SqliteTaskRepo;
pub use task_log::SqliteTaskLogRepo;
pub use task_run::SqliteTaskRunRepo;
pub use tenant::SqliteTenantRepo;
pub use user::SqliteUserRepo;
