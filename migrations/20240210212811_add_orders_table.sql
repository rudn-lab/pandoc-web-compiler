-- Add migration script here

CREATE TABLE orders (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL REFERENCES accounts(id),
    created_at_unix_time INTEGER NOT NULL,
    is_on_disk BOOLEAN NOT NULL,
    is_running BOOLEAN NOT NULL,
    status_json TEXT -- null if running
);
