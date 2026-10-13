export interface FrozenNavigationMenu {
  id: string;
  version: number;
  parent_id: string | null;
  name: string;
  path: string;
  additional_permission: string | null;
  visible: boolean;
  sort_order: number;
  icon: string | null;
}

export interface NavigationAssignment {
  id: string;
  tenant_id: string;
  version: number;
  status: 'active' | 'revoked';
  template: {
    id: string;
    version: number;
    name: string;
    edition: string | null;
    validity_days: number | null;
    menus: FrozenNavigationMenu[];
  };
  assigned_by: string;
  assigned_at: string;
  expires_at: string | null;
  revoked_by: string | null;
  revoked_at: string | null;
}

export interface NavigationDecisionResult {
  assignment: NavigationAssignment;
  receipt_id: string;
  replayed: boolean;
}

export interface NavigationNode {
  id: string;
  menu_version: number | null;
  parent_id: string | null;
  name: string;
  path: string;
  sort_order: number;
  icon: string | null;
  source: 'catalog' | 'template';
  recovery: boolean;
}

export interface NavigationView {
  tenant_id: string;
  mode: 'default_catalog' | 'assigned' | 'expired';
  assignment_id: string | null;
  assignment_version: number;
  template_id: string | null;
  template_version: number | null;
  expires_at: string | null;
  nodes: NavigationNode[];
  filtered_count: number;
  observed_at: string;
  affects_navigation_only: boolean;
}

export interface NavigationHistoryEntry {
  id: string;
  assignment_id: string;
  action: 'assign' | 'revoke';
  actor_id: string;
  assignment_version: number;
  result: NavigationAssignment;
  created_at: string;
}

export interface NavigationHistoryPage {
  data: NavigationHistoryEntry[];
  total: number;
}

export interface AssignNavigationRequest {
  expected_template_version: number;
  expected_assignment_version: number;
  idempotency_key: string;
  expires_at: string | null;
}

export interface RevokeNavigationRequest {
  assignment_id: string;
  expected_assignment_version: number;
  idempotency_key: string;
  reason: string;
}
