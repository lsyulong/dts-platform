import { describe, it, expect } from 'vitest';
import { loginSchema, datasourceSchema, taskCreateSchema, getFirstError } from '../validations';

describe('loginSchema', () => {
  it('accepts valid login data', () => {
    const result = loginSchema.safeParse({
      tenantSlug: 'default',
      username: 'admin',
      password: 'password123',
    });
    expect(result.success).toBe(true);
  });

  it('rejects empty username', () => {
    const result = loginSchema.safeParse({
      tenantSlug: 'default',
      username: '',
      password: 'pass',
    });
    expect(result.success).toBe(false);
    expect(getFirstError(result)).toBe('required');
  });

  it('rejects empty password', () => {
    const result = loginSchema.safeParse({
      tenantSlug: 'default',
      username: 'admin',
      password: '',
    });
    expect(result.success).toBe(false);
  });

  it('rejects empty tenantSlug', () => {
    const result = loginSchema.safeParse({
      tenantSlug: '',
      username: 'admin',
      password: 'pass',
    });
    expect(result.success).toBe(false);
  });
});

describe('datasourceSchema', () => {
  it('accepts SQLite paths without a network port', () => {
    expect(
      datasourceSchema.safeParse({
        name: 'local test',
        db_type: 'SQLite',
        host: ':memory:',
        port: 0,
      }).success,
    ).toBe(true);
  });

  it('still requires a positive port for network databases', () => {
    expect(
      datasourceSchema.safeParse({
        name: 'network test',
        db_type: 'MySQL',
        host: 'localhost',
        port: 0,
      }).success,
    ).toBe(false);
  });
  const validInput = {
    name: 'Production DB',
    db_type: 'MySQL' as const,
    host: '192.168.1.100',
    port: 3306,
    username: 'admin',
    password: 'secret',
    database_name: 'mydb',
  };

  it('accepts valid datasource data', () => {
    const result = datasourceSchema.safeParse(validInput);
    expect(result.success).toBe(true);
  });

  it('requires a catalog for DuckDB through SwanLake', () => {
    expect(
      datasourceSchema.safeParse({
        name: 'warehouse',
        db_type: 'DuckDB',
        host: 'swanlake.internal',
        port: 4214,
        database_name: '',
      }).success,
    ).toBe(false);
    expect(
      datasourceSchema.safeParse({
        name: 'warehouse',
        db_type: 'DuckDB',
        host: 'swanlake.internal',
        port: 4214,
        database_name: 'warehouse',
      }).success,
    ).toBe(true);
  });

  it('accepts minimal valid data (only required fields)', () => {
    const result = datasourceSchema.safeParse({
      name: 'My DB',
      db_type: 'PostgreSQL',
      host: 'localhost',
      port: 5432,
    });
    expect(result.success).toBe(true);
  });

  it('rejects empty name', () => {
    const result = datasourceSchema.safeParse({ ...validInput, name: '' });
    expect(result.success).toBe(false);
  });

  it('rejects invalid db_type', () => {
    const result = datasourceSchema.safeParse({
      ...validInput,
      db_type: 'InvalidDB',
    });
    expect(result.success).toBe(false);
  });

  it('rejects empty host', () => {
    const result = datasourceSchema.safeParse({ ...validInput, host: '' });
    expect(result.success).toBe(false);
  });

  it('rejects port 0', () => {
    const result = datasourceSchema.safeParse({ ...validInput, port: 0 });
    expect(result.success).toBe(false);
  });

  it('rejects port > 65535', () => {
    const result = datasourceSchema.safeParse({ ...validInput, port: 99999 });
    expect(result.success).toBe(false);
  });

  it('defaults optional fields to empty strings', () => {
    const result = datasourceSchema.safeParse({
      name: 'Test',
      db_type: 'MySQL',
      host: 'localhost',
      port: 3306,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.username).toBe('');
      expect(result.data.password).toBe('');
      expect(result.data.database_name).toBe('');
    }
  });
});

describe('taskCreateSchema', () => {
  const validInput = {
    name: 'Sync Task',
    description: 'Test task',
    source_id: 'ds-1',
    target_id: 'ds-2',
    cron_expr: '0 */6 * * *',
    table_mappings: [{ source_table: 'customers', target_table: 'customers' }],
  };

  it('accepts valid task data', () => {
    const result = taskCreateSchema.safeParse(validInput);
    expect(result.success).toBe(true);
  });

  it('accepts minimal valid data without optional fields', () => {
    const result = taskCreateSchema.safeParse({
      name: 'My Task',
      source_id: 'ds-1',
      target_id: 'ds-2',
      table_mappings: [{ source_table: 'customers', target_table: 'customers' }],
    });
    expect(result.success).toBe(true);
  });

  it('rejects empty name', () => {
    const result = taskCreateSchema.safeParse({ ...validInput, name: '' });
    expect(result.success).toBe(false);
  });

  it('rejects empty source_id', () => {
    const result = taskCreateSchema.safeParse({
      ...validInput,
      source_id: '',
    });
    expect(result.success).toBe(false);
  });

  it('rejects empty target_id', () => {
    const result = taskCreateSchema.safeParse({
      ...validInput,
      target_id: '',
    });
    expect(result.success).toBe(false);
  });

  it('rejects missing mappings, duplicate targets, same datasource and unsupported CDC mode', () => {
    expect(taskCreateSchema.safeParse({ ...validInput, table_mappings: [] }).success).toBe(false);
    expect(
      taskCreateSchema.safeParse({
        ...validInput,
        table_mappings: [
          { source_table: 'customers', target_table: 'customers' },
          { source_table: 'accounts', target_table: 'customers' },
        ],
      }).success,
    ).toBe(false);
    expect(taskCreateSchema.safeParse({ ...validInput, target_id: 'ds-1' }).success).toBe(false);
    expect(taskCreateSchema.safeParse({ ...validInput, sync_mode: 'cdc' }).success).toBe(false);
  });

  it('rejects whitespace-only identifiers and invalid batch size', () => {
    expect(
      taskCreateSchema.safeParse({
        ...validInput,
        table_mappings: [{ source_table: ' ', target_table: 'customers' }],
      }).success,
    ).toBe(false);
    expect(taskCreateSchema.safeParse({ ...validInput, batch_size: 0 }).success).toBe(false);
    expect(taskCreateSchema.safeParse({ ...validInput, batch_size: 1.5 }).success).toBe(false);
  });
});

describe('getFirstError', () => {
  it('returns null for successful parse', () => {
    const result = loginSchema.safeParse({
      tenantSlug: 'default',
      username: 'admin',
      password: 'pass',
    });
    expect(getFirstError(result)).toBeNull();
  });

  it('returns first error message for failed parse', () => {
    const result = datasourceSchema.safeParse({
      name: '',
      db_type: 'MySQL',
      host: '',
      port: 0,
    });
    expect(getFirstError(result)).toBe('required');
  });
});
