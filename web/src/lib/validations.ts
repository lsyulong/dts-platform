import { z } from 'zod/v4';

const DB_TYPES = ['MySQL', 'PostgreSQL', 'MongoDB', 'Redis', 'Kafka', 'ClickHouse', 'Oracle', 'SQLServer'] as const;

// ── Login ──────────────────────────────────────────────────────────
export const loginSchema = z.object({
  tenantSlug: z.string().min(1, 'required').max(100, 'too_long'),
  username: z.string().min(1, 'required').max(100, 'too_long'),
  password: z.string().min(1, 'required').max(128, 'too_long'),
});

export type LoginFormData = z.infer<typeof loginSchema>;

// ── Datasource ─────────────────────────────────────────────────────
export const datasourceSchema = z.object({
  name: z.string().min(1, 'required').max(100, 'too_long'),
  db_type: z.enum(DB_TYPES),
  host: z.string().min(1, 'required').max(255, 'too_long'),
  port: z.number().int().min(1).max(65535),
  username: z.string().max(100).optional().default(''),
  password: z.string().max(128).optional().default(''),
  database_name: z.string().max(100).optional().default(''),
});

export type DatasourceFormData = z.infer<typeof datasourceSchema>;

// ── Task Create ────────────────────────────────────────────────────
export const taskCreateSchema = z.object({
  name: z.string().min(1, 'required').max(200, 'too_long'),
  description: z.string().max(500).optional().default(''),
  source_id: z.string().min(1, 'required'),
  target_id: z.string().min(1, 'required'),
  cron_expr: z.string().max(100).optional(),
});

export type TaskCreateFormData = z.infer<typeof taskCreateSchema>;

// ── Helper ─────────────────────────────────────────────────────────
/**
 * Returns the first validation error message, or null if valid.
 */
export function getFirstError(result: { success: true } | { success: false; error: z.ZodError }): string | null {
  if (result.success) return null;
  return result.error.issues[0]?.message ?? null;
}
