-- Add migration script here
CREATE TABLE accounts (
    id INTEGER NOT NULL PRIMARY KEY,
    user_name TEXT NOT NULL,
    token TEXT NOT NULL,
    balance REAL NOT NULL
);

CREATE INDEX account_token ON accounts(token);