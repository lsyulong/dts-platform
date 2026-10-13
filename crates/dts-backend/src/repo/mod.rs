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
pub mod assets;
pub mod bi_share;
pub mod data_access;
pub mod datasource;
pub mod development_revisions;
pub mod master_data;
pub mod message_delivery;
pub mod metadata_catalog;
pub mod metadata_lineage;
pub mod monitoring;
pub mod navigation;
pub mod notification_connection;
pub mod organization_members;
pub mod platform_projection;
pub mod product_job;
pub mod product_resource;
pub mod quality_alerts;
pub mod role;
pub mod runtime_configuration;
pub mod seed;
pub mod sensitive_findings;
pub mod service_access;
pub mod standard_governance;
pub mod tag_assignment;
pub mod task;
pub mod task_log;
pub mod task_run;
pub mod tenant;
pub mod user;

pub use alert::AlertRepo;
pub use alert_policy::AlertPolicyRepo;
pub use assets::AssetsRepo;
pub use bi_share::BiShareRepo;
pub use data_access::DataAccessRepo;
pub use datasource::{
    DatasourceProbe, DatasourceRepo, RawConnectionInfo, RecordDatasourceProbeParams,
};
pub use development_revisions::DevelopmentRevisionRepo;
pub use master_data::MasterDataRepo;
pub use message_delivery::MessageDeliveryRepo;
pub use metadata_catalog::MetadataCatalogRepo;
pub use metadata_lineage::MetadataLineageRepo;
pub use monitoring::MonitoringRepo;
pub use navigation::NavigationRepo;
pub use notification_connection::*;
pub use organization_members::OrganizationMembersRepo;
pub use platform_projection::PlatformProjectionRepo;
pub use product_job::ProductJobRepo;
pub use product_resource::*;
pub use quality_alerts::QualityAlertRepo;
pub use role::RoleRepo;
pub use runtime_configuration::RuntimeConfigurationRepo;
pub use seed::SeedRepo;
pub use sensitive_findings::SensitiveFindingRepo;
pub use service_access::ServiceAccessRepo;
pub use standard_governance::StandardGovernanceRepo;
pub use tag_assignment::TagAssignmentRepo;
pub use task::TaskRepo;
pub use task_log::TaskLogRepo;
pub use task_run::{CreateTaskRunParams, TaskRunRepo, UpdateTaskRunParams};
pub use tenant::TenantRepo;
pub use user::UserRepo;
