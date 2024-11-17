-- Add migration script here
CREATE TABLE IF NOT EXISTS "post" (
    "uri" varchar primary key,
    "cid" varchar not null,
    "author" varchar not null,
    "indexedAt" varchar not null
);
CREATE TABLE IF NOT EXISTS "app_state" (
    "key" varchar primary key,
    "value" varchar not null
);