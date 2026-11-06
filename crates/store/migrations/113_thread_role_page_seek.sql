-- Seek the summary by thread on every write, and repair a ghost row.
--
-- Migration 112's triggers cleared a thread's rows with
-- `WHERE (account_id, thread_id) IN (SELECT ... FROM messages ...)`. The
-- summary is WITHOUT ROWID with PRIMARY KEY (role, account_id, thread_id), so
-- that predicate could not seek: every message and label write scanned the
-- whole summary table (a 27k-mailbox 500-message sync page went 134 ms to
-- 2.3 s CPU; a 200-message star 6 ms to 2.1 s). The delete/update triggers
-- also resolved the thread from `messages` *after* the change, so deleting a
-- thread's last message matched nothing and left a permanent row.
--
-- This migration adds the (account_id, thread_id) index and replaces the six
-- triggers with direct key deletes. Membership, order and latest_date are
-- unchanged; 113 does not touch the rows themselves.

CREATE INDEX IF NOT EXISTS idx_thread_role_pages_thread
    ON thread_role_pages(account_id, thread_id);

DROP TRIGGER IF EXISTS thread_role_pages_message_insert;
DROP TRIGGER IF EXISTS thread_role_pages_message_update;
DROP TRIGGER IF EXISTS thread_role_pages_message_delete;
DROP TRIGGER IF EXISTS thread_role_pages_label_insert;
DROP TRIGGER IF EXISTS thread_role_pages_label_delete;
DROP TRIGGER IF EXISTS thread_role_pages_label_role_update;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_insert
AFTER INSERT ON messages
BEGIN
    DELETE FROM thread_role_pages
     WHERE account_id = NEW.account_id AND thread_id = NEW.thread_id;
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
     WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id;
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
     WHERE account_id = NEW.account_id AND thread_id = NEW.thread_id;
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
     WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id;
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
     WHERE account_id = (SELECT account_id FROM messages WHERE id = NEW.message_id)
       AND thread_id = (SELECT thread_id FROM messages WHERE id = NEW.message_id);
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
     WHERE account_id = (SELECT account_id FROM messages WHERE id = OLD.message_id)
       AND thread_id = (SELECT thread_id FROM messages WHERE id = OLD.message_id);
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
