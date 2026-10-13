import type { ProductResource } from '../resource-types';

export interface OrganizationUser {
  id: string;
  tenant_id: string;
  username: string;
  display_name: string | null;
  status: string;
}
export interface OrganizationMembership {
  id: string;
  tenant_id: string;
  user_id: string;
  organization_id: string | null;
  version: number;
  changed_by: string;
  changed_at: string;
}
export interface UserOrganizationMembership {
  user: OrganizationUser;
  membership: OrganizationMembership | null;
}
export interface OrganizationMember {
  user: OrganizationUser;
  membership: OrganizationMembership;
  is_manager: boolean;
}
export interface OrganizationDetails {
  organization: ProductResource;
  manager: OrganizationMember | null;
  manager_valid: boolean;
  member_count: number;
  children_count: number;
  legacy_membership_ignored: boolean;
}
export interface OrganizationMemberPage {
  data: OrganizationMember[];
  total: number;
}
export interface OrganizationIdentity {
  id: string;
  code: string;
  name: string;
  version: number;
  parent_id: string | null;
}
export interface OrganizationDecisionResult {
  membership: OrganizationMembership | null;
  source: OrganizationIdentity | null;
  target: OrganizationIdentity | null;
  manager_changes: {
    organization: OrganizationIdentity;
    previous_user_id: string | null;
    manager: OrganizationUser | null;
  }[];
  receipt_id: string;
  replayed: boolean;
  roles_unchanged: boolean;
}
export interface OrganizationHistoryPage {
  data: {
    id: string;
    action: 'transfer' | 'remove' | 'set_manager';
    actor_id: string;
    user_id: string | null;
    reason: string;
    result: OrganizationDecisionResult;
    created_at: string;
  }[];
  total: number;
}
export interface ManagerHandover {
  expected_organization_version: number;
  successor_user_id: string | null;
}
export interface TransferMemberRequest {
  expected_membership_version: number;
  target_organization_id: string;
  expected_target_version: number;
  reason: string;
  idempotency_key: string;
  manager_handover: ManagerHandover | null;
}
export interface RemoveMemberRequest {
  expected_membership_version: number;
  reason: string;
  idempotency_key: string;
  manager_handover: ManagerHandover | null;
}
export interface SetManagerRequest {
  expected_organization_version: number;
  manager_user_id: string | null;
  reason: string;
  idempotency_key: string;
}
