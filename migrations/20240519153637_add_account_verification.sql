-- Add migration script here
ALTER TABLE accounts ADD COLUMN verification_method TINYINT DEFAULT NULL;