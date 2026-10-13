CREATE UNIQUE INDEX IF NOT EXISTS uq_model_physical_table
ON product_resource(tenant_id, lower(CASE
    WHEN instr(json_extract(properties,'$.table_name'),'.') > 0
    THEN json_extract(properties,'$.table_name')
    ELSE 'main.' || json_extract(properties,'$.table_name') END))
WHERE kind='data_model';

CREATE UNIQUE INDEX IF NOT EXISTS uq_warehouse_layer_prefix
ON product_resource(tenant_id, lower(json_extract(properties,'$.prefix')))
WHERE kind='warehouse_layer';
