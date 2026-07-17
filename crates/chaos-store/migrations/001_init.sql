CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY NOT NULL,
  username TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS subscriptions (
  id TEXT PRIMARY KEY NOT NULL,
  tag TEXT,
  url TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'ok'
);

CREATE TABLE IF NOT EXISTS nodes (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  tag TEXT,
  link TEXT NOT NULL,
  protocol TEXT,
  address TEXT,
  subscription_id TEXT REFERENCES subscriptions(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS latency_results (
  node_id TEXT PRIMARY KEY NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  latency_ms INTEGER,
  alive INTEGER NOT NULL,
  tested_at TEXT NOT NULL,
  message TEXT
);
