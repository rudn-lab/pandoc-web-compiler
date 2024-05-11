-- Add migration script here
ALTER TABLE orders ADD COLUMN src_file_list TEXT NOT NULL DEFAULT "[]";