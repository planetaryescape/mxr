CREATE TABLE IF NOT EXISTS rule_category_sources (
 message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
 category_id TEXT NOT NULL REFERENCES personal_categories(id) ON DELETE CASCADE,
 rule_id TEXT NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
 assigned INTEGER NOT NULL CHECK (assigned IN (0,1)),
 priority INTEGER NOT NULL,
 rule_name TEXT NOT NULL,
 rule_revision TEXT NOT NULL,
 PRIMARY KEY(message_id,category_id,rule_id)
);
CREATE INDEX IF NOT EXISTS rule_category_sources_rule ON rule_category_sources(rule_id,message_id);
CREATE VIEW IF NOT EXISTS effective_category_memberships AS
WITH pairs AS (
 SELECT message_id,category_id FROM message_category_memberships
 UNION SELECT message_id,category_id FROM rule_category_sources
), winners AS (
 SELECT *, ROW_NUMBER() OVER (PARTITION BY message_id,category_id ORDER BY priority,rule_id) AS position FROM rule_category_sources
)
SELECT p.message_id,p.category_id,COALESCE(m.manual_decision,w.assigned,0) AS assigned,
 COALESCE(m.revision,0) AS revision
FROM pairs p LEFT JOIN message_category_memberships m USING(message_id,category_id)
LEFT JOIN winners w ON w.message_id=p.message_id AND w.category_id=p.category_id AND w.position=1;
