import { z } from 'zod/v4';

const DB_TYPES = [
  'MySQL',
  'PostgreSQL',
  'MongoDB',
  'Redis',
  'Kafka',
  'ClickHouse',
  'Oracle',
  'SQLServer',
  'SQLite',
  'DuckDB',
] as const;

// ── Login ──────────────────────────────────────────────────────────
export const loginSchema = z.object({
  tenantSlug: z.string().min(1, 'required').max(100, 'too_long'),
  username: z.string().min(1, 'required').max(100, 'too_long'),
  password: z.string().min(1, 'required').max(128, 'too_long'),
});

export type LoginFormData = z.infer<typeof loginSchema>;

// ── Datasource ─────────────────────────────────────────────────────
export const datasourceSchema = z
  .object({
    name: z.string().min(1, 'required').max(100, 'too_long'),
    db_type: z.enum(DB_TYPES),
    host: z.string().min(1, 'required').max(255, 'too_long'),
    port: z.number().int().min(0).max(65535),
    username: z.string().max(100).optional().default(''),
    password: z.string().max(128).optional().default(''),
    database_name: z.string().max(100).optional().default(''),
  })
  .refine((value) => value.db_type === 'SQLite' || value.port > 0, {
    path: ['port'],
    message: 'required',
  })
  .refine((value) => value.db_type !== 'DuckDB' || value.database_name.trim().length > 0, {
    path: ['database_name'],
    message: 'required',
  })
  .refine((value) => value.db_type !== 'SQLite' || value.port === 0, {
    path: ['port'],
    message: 'invalid',
  });

export type DatasourceFormData = z.infer<typeof datasourceSchema>;

// ── Task Create ────────────────────────────────────────────────────
const optionalIdentifier = z
  .string()
  .trim()
  .max(255, 'too_long')
  .optional()
  .transform((value) => value || undefined);

export const tableMappingSchema = z.object({
  source_table: z.string().trim().min(1, 'required').max(255, 'too_long'),
  target_table: z.string().trim().min(1, 'required').max(255, 'too_long'),
  source_schema: optionalIdentifier,
  target_schema: optionalIdentifier,
});

export type TableMapping = z.infer<typeof tableMappingSchema>;

export const createEmptyTableMapping = (): TableMapping => ({
  source_table: '',
  target_table: '',
  source_schema: '',
  target_schema: '',
});

export const tableMappingsSchema = z
  .array(tableMappingSchema)
  .min(1, 'required')
  .max(100)
  .refine((mappings) => {
    const targets = mappings.map((mapping) =>
      JSON.stringify([mapping.target_schema ?? '', mapping.target_table]),
    );
    return new Set(targets).size === targets.length;
  }, 'duplicate_target');

export const taskCreateSchema = z
  .object({
    name: z.string().trim().min(1, 'required').max(200, 'too_long'),
    description: z.string().max(500).optional().default(''),
    source_id: z.string().min(1, 'required'),
    target_id: z.string().min(1, 'required'),
    cron_expr: z.string().max(100).optional(),
    table_mappings: tableMappingsSchema,
    sync_mode: z.literal('full').default('full'),
    batch_size: z.number().int().min(1).max(100000).optional(),
  })
  .refine((task) => task.source_id !== task.target_id, 'same_datasource');

export type TaskCreateFormData = z.infer<typeof taskCreateSchema>;

// ── Helper ─────────────────────────────────────────────────────────
/**
 * Returns the first validation error message, or null if valid.
 */
export function getFirstError(
  result: { success: true } | { success: false; error: z.ZodError },
): string | null {
  if (result.success) return null;
  return result.error.issues[0]?.message ?? null;
}
