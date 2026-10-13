import { createContext } from 'react';
import type { NavigationPolicyState } from './policy';

export const NavigationContext = createContext<NavigationPolicyState | null>(null);
