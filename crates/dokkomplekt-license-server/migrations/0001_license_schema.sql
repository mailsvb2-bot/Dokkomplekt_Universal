CREATE TABLE IF NOT EXISTS license_orders (
    id UUID PRIMARY KEY,
    plan TEXT NOT NULL,
    amount_rub BIGINT NOT NULL CHECK (amount_rub > 0),
    status TEXT NOT NULL,
    machine_hash TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS billing_events (
    id UUID PRIMARY KEY,
    order_id UUID NOT NULL REFERENCES license_orders(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL,
    provider_event_id TEXT NOT NULL,
    provider_reference_id TEXT,
    status TEXT NOT NULL,
    amount_rub BIGINT NOT NULL CHECK (amount_rub > 0),
    received_at TIMESTAMPTZ NOT NULL,
    UNIQUE (provider, provider_event_id)
);

CREATE INDEX IF NOT EXISTS idx_billing_events_order_id ON billing_events(order_id);

CREATE TABLE IF NOT EXISTS license_documents (
    id UUID PRIMARY KEY,
    order_id UUID NOT NULL REFERENCES license_orders(id) ON DELETE RESTRICT,
    license_id TEXT NOT NULL UNIQUE,
    document_json TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS license_machines (
    id UUID PRIMARY KEY,
    order_id UUID NOT NULL REFERENCES license_orders(id) ON DELETE RESTRICT,
    machine_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (order_id, machine_hash)
);

CREATE TABLE IF NOT EXISTS license_audit_events (
    id UUID PRIMARY KEY,
    entity_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    happened_at TIMESTAMPTZ NOT NULL,
    details_json TEXT NOT NULL
);
