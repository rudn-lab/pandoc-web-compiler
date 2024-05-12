-- Add migration script here
CREATE TABLE promocodes (
    id INTEGER NOT NULL PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    money_value INTEGER NOT NULL,
    created_at_unix_time INTEGER NOT NULL,
    claimed_by INTEGER REFERENCES accounts(id), -- null
    claimed_at_unix_time INTEGER -- null
);

CREATE INDEX promocodes_code ON promocodes(code);