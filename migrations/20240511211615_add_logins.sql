-- Add migration script here
CREATE TABLE logins (
    handle TEXT NOT NULL PRIMARY KEY,
    password_hash TEXT NOT NULL,
    account_id INTEGER UNIQUE NOT NULL REFERENCES accounts(id)
);