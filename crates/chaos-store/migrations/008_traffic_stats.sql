-- Traffic statistics for reporting
CREATE TABLE traffic_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id TEXT,
    timestamp TEXT NOT NULL,
    bytes_up INTEGER NOT NULL DEFAULT 0,
    bytes_down INTEGER NOT NULL DEFAULT 0,
    connections INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_traffic_stats_node ON traffic_stats(node_id);
CREATE INDEX idx_traffic_stats_time ON traffic_stats(timestamp);
