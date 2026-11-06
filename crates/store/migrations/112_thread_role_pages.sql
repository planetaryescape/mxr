-- Ordered thread pages for the "all accounts" system mailboxes.
--
-- `list_threads_by_role` used to group every message of the account set by
-- thread and order by MAX(date), so a page of 150 could not stop early:
-- SQLite scanned the whole mailbox through `idx_messages_thread` and built a
-- temp B-tree for the ORDER BY. On a 27k-message profile that was 37 CPU ms
-- per page and grew with history.
--
-- This table holds, for each (role, account, thread) a thread belongs to, the
-- newest date of the thread's messages in that role, clamped exactly as the
-- old ordering was (`date > now + 1 day` sorts as 0). The page query is an
-- ordered range scan of `idx_thread_role_pages_page` and stops at LIMIT.
--
-- The role is the same rule the reads apply: All Mail is every message,
-- Starred is the message flag (IMAP stars are a flag, not a label), and every
-- other role is the stored `labels.role` of a System label, or its name for a
-- label that carries no role. Membership is recomputed from `messages` and
-- `message_labels` by the triggers below, inside the same transaction as the
-- write that changed them, so a page never reads a stale set. No view is used:
-- a view over `labels` would block the backup path's column-drop repairs on
-- older schemas.
CREATE TABLE IF NOT EXISTS thread_role_pages (
    role        TEXT NOT NULL,
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id   TEXT NOT NULL,
    latest_date INTEGER NOT NULL,
    PRIMARY KEY (role, account_id, thread_id)
) WITHOUT ROWID;

-- The page scan: role first, then the page order. `account_id` is a filter
-- column, not part of the order, so several accounts share one ordered scan
-- instead of one index per account and a merge.
CREATE INDEX IF NOT EXISTS idx_thread_role_pages_page
    ON thread_role_pages(role, latest_date DESC, thread_id DESC);

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_insert
AFTER INSERT ON messages
BEGIN
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id);
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
END;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_update
AFTER UPDATE OF account_id, thread_id, date, flags ON messages
WHEN OLD.account_id IS NOT NEW.account_id
  OR OLD.thread_id IS NOT NEW.thread_id
  OR OLD.date IS NOT NEW.date
  OR OLD.flags IS NOT NEW.flags
BEGIN
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id);
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id);
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE m.account_id = NEW.account_id AND m.thread_id = NEW.thread_id AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
END;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_delete
AFTER DELETE ON messages
BEGIN
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id);
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE m.account_id = OLD.account_id AND m.thread_id = OLD.thread_id AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
END;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_label_insert
AFTER INSERT ON message_labels
BEGIN
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = NEW.message_id));
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = NEW.message_id)
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = NEW.message_id) AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = NEW.message_id) AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
END;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_label_delete
AFTER DELETE ON message_labels
BEGIN
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = OLD.message_id));
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = OLD.message_id)
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = OLD.message_id) AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE (m.account_id, m.thread_id) IN (SELECT account_id, thread_id FROM messages
         WHERE id = OLD.message_id) AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
END;

-- A label's role, kind or name changing (a provider re-reports the role, or a
-- sync backfills it from the name) re-places every thread that carries it.
CREATE TRIGGER IF NOT EXISTS thread_role_pages_label_role_update
AFTER UPDATE OF role, kind, name ON labels
WHEN OLD.role IS NOT NEW.role OR OLD.kind IS NOT NEW.kind OR OLD.name IS NOT NEW.name
BEGIN
    DELETE FROM thread_role_pages
     WHERE (account_id, thread_id) IN (
        SELECT m.account_id, m.thread_id FROM messages m WHERE EXISTS (SELECT 1 FROM message_labels mll
         WHERE mll.label_id = NEW.id AND mll.message_id IN (
            SELECT id FROM messages WHERE account_id = m.account_id
              AND thread_id = m.thread_id)));
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE EXISTS (SELECT 1 FROM message_labels mll
         WHERE mll.label_id = NEW.id AND mll.message_id IN (
            SELECT id FROM messages WHERE account_id = m.account_id
              AND thread_id = m.thread_id))
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE EXISTS (SELECT 1 FROM message_labels mll
         WHERE mll.label_id = NEW.id AND mll.message_id IN (
            SELECT id FROM messages WHERE account_id = m.account_id
              AND thread_id = m.thread_id)) AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id;
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
      JOIN message_labels ml ON ml.message_id = m.id
      JOIN labels l ON l.id = ml.label_id
     WHERE EXISTS (SELECT 1 FROM message_labels mll
         WHERE mll.label_id = NEW.id AND mll.message_id IN (
            SELECT id FROM messages WHERE account_id = m.account_id
              AND thread_id = m.thread_id)) AND l.kind = 'system'
       AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
     GROUP BY 1, m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
END;

-- Existing rows: one pass over the mailbox builds the summary. Runs once,
-- when this migration is stamped; new writes keep it current from then on.
INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
SELECT 'all_mail', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
  FROM messages m
 GROUP BY m.account_id, m.thread_id
ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;

INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
SELECT 'starred', m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
  FROM messages m
 WHERE (m.flags & 2) != 0
 GROUP BY m.account_id, m.thread_id
ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;

INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
SELECT COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END), m.account_id, m.thread_id, MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
  FROM messages m
  JOIN message_labels ml ON ml.message_id = m.id
  JOIN labels l ON l.id = ml.label_id
 WHERE l.kind = 'system' AND COALESCE(l.role, CASE lower(l.name)
               WHEN 'inbox' THEN 'inbox'
               WHEN 'sent' THEN 'sent'
               WHEN 'draft' THEN 'drafts'
               WHEN 'drafts' THEN 'drafts'
               WHEN 'trash' THEN 'trash'
               WHEN 'spam' THEN 'spam'
               WHEN 'all' THEN 'all_mail'
               WHEN 'all mail' THEN 'all_mail'
               WHEN 'starred' THEN 'starred'
           END) NOT IN ('all_mail', 'starred')
 GROUP BY 1, m.account_id, m.thread_id
ON CONFLICT(role, account_id, thread_id) DO UPDATE SET latest_date = excluded.latest_date;
