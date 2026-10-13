import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import {
  isNavigationSession,
  useNavigationSession,
  type NavigationSession,
} from '../navigation/session';
import type {
  OrganizationDetails,
  OrganizationMemberPage,
  UserOrganizationMembership,
} from './types';

export { useNavigationSession as useOrganizationSession };
export type OrganizationSession = NavigationSession;
export const organizationQueryKey = (session: OrganizationSession) => [
  'organization-members',
  session.key,
];
export function useOrganizationQuery<T>(
  path: string,
  params: Record<string, string | number>,
  enabled = true,
  validate?: (data: T, tenant: string) => void,
) {
  const session = useNavigationSession();
  return useQuery({
    queryKey: [...organizationQueryKey(session), path, params],
    enabled: session.ready && session.canReadDefinitions && enabled,
    retry: false,
    staleTime: 0,
    queryFn: async ({ signal }) => {
      const response = await client.get<T>(path, {
        params,
        signal,
        headers: { Authorization: `Bearer ${session.token}` },
      });
      if (!isNavigationSession(session)) throw new Error('Organization session changed');
      validate?.(response.data, session.tenantId ?? '');
      return response.data;
    },
  });
}
export const organizationPath = (id: string) => `/product/organizations/${encodeURIComponent(id)}`;
export const membershipPath = (id: string) =>
  `/product/organization-members/users/${encodeURIComponent(id)}`;
export function useOrganizationDetails(id: string) {
  return useOrganizationQuery<OrganizationDetails>(
    organizationPath(id),
    {},
    !!id,
    (data, tenant) => {
      if (
        data.organization.tenant_id !== tenant ||
        data.organization.id !== id ||
        data.organization.kind !== 'organization' ||
        (data.manager && data.manager.user.tenant_id !== tenant)
      )
        throw new Error('Invalid organization response');
    },
  );
}
export function useUserMembership(id: string) {
  return useOrganizationQuery<UserOrganizationMembership>(
    membershipPath(id),
    {},
    !!id,
    (data, tenant) => {
      if (
        data.user.tenant_id !== tenant ||
        data.user.id !== id ||
        (data.membership &&
          (data.membership.tenant_id !== tenant || data.membership.user_id !== id))
      )
        throw new Error('Invalid membership response');
    },
  );
}
export function useOrganizationMembers(id: string, q = '', offset = 0) {
  return useOrganizationQuery<OrganizationMemberPage>(
    `${organizationPath(id)}/members`,
    { q, limit: 20, offset },
    !!id,
    (data, tenant) => {
      if (
        !Array.isArray(data.data) ||
        data.data.some(
          (member) =>
            member.user.tenant_id !== tenant ||
            member.membership.tenant_id !== tenant ||
            member.membership.organization_id !== id,
        )
      )
        throw new Error('Invalid organization members');
    },
  );
}
