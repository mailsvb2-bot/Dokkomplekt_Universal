ALTER TABLE license_orders
    ADD COLUMN IF NOT EXISTS access_token_hash TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'license_orders_access_token_hash_format'
    ) THEN
        ALTER TABLE license_orders
            ADD CONSTRAINT license_orders_access_token_hash_format
            CHECK (
                access_token_hash IS NULL
                OR access_token_hash ~ '^[0-9a-f]{64}$'
            );
    END IF;
END
$$;
