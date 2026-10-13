import type { ReactNode } from 'react';
import { NavigationContext } from './context';
import { useNavigationPolicy } from './policy';

export function NavigationProvider({ children }: { children: ReactNode }) {
  const policy = useNavigationPolicy();
  return <NavigationContext.Provider value={policy}>{children}</NavigationContext.Provider>;
}
