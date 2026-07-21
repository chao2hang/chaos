-- Subscription auto-refresh scheduling
ALTER TABLE subscriptions ADD COLUMN refresh_interval_hours INTEGER NOT NULL DEFAULT 0;
ALTER TABLE subscriptions ADD COLUMN last_refreshed_at TEXT;
ALTER TABLE subscriptions ADD COLUMN next_refresh_at TEXT;
