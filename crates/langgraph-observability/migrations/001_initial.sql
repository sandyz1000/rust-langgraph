-- Initial migration for LangGraph Observability
CREATE TABLE IF NOT EXISTS graph_runs (
    id TEXT PRIMARY KEY,
    graph_id TEXT NOT NULL,
    status TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT,
    initial_state TEXT,
    final_state TEXT,
    error TEXT,
    metadata TEXT,
    config TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS trace_spans (
    id TEXT PRIMARY KEY,
    trace_id TEXT NOT NULL,
    parent_span_id TEXT,
    operation_name TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT,
    status TEXT NOT NULL,
    tags TEXT,
    events TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_graph_runs_graph_id ON graph_runs(graph_id);
CREATE INDEX IF NOT EXISTS idx_graph_runs_status ON graph_runs(status);
CREATE INDEX IF NOT EXISTS idx_trace_spans_trace_id ON trace_spans(trace_id);
CREATE INDEX IF NOT EXISTS idx_trace_spans_parent_id ON trace_spans(parent_span_id);
