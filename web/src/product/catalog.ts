import { accessGroups, accessModules } from './access-catalog';
import { platformGroups, platformModules } from './platform-catalog';

export type { ProductGroup, ProductModule, ProductText, ResourceField } from './catalog-types';
export const productGroups = [...platformGroups, ...accessGroups];
export const productModules = [...platformModules, ...accessModules];

export const getProductModule = (id: string) => productModules.find((module) => module.id === id);
export const getProductModuleByPath = (path: string) =>
  productModules.find((module) => module.path === path);
