CREATE UNIQUE INDEX IF NOT EXISTS uq_model_physical_table
ON product_resource(tenant_id, lower(CASE
    WHEN strpos(properties->>'table_name','.') > 0
    THEN properties->>'table_name'
    ELSE 'main.' || (properties->>'table_name') END))
WHERE kind='data_model';

CREATE UNIQUE INDEX IF NOT EXISTS uq_warehouse_layer_prefix
ON product_resource(tenant_id, lower(properties->>'prefix'))
WHERE kind='warehouse_layer';
