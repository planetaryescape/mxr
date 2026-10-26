-- User corrections are durable state, unlike rebuildable assessment caches.
CREATE TABLE IF NOT EXISTS todo_corrections (
    id TEXT PRIMARY KEY,
    todo_id TEXT NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
    reason TEXT NOT NULL CHECK(reason IN ('not_a_todo', 'not_mine', 'already_done', 'wrong_details')),
    before_json TEXT NOT NULL,
    after_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    undone_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_todo_corrections_todo ON todo_corrections(todo_id);
