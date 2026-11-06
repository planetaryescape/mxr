-- Keep new mail and new labels cheap inside a long thread.
--
-- After 113, a write no longer scans the summary, but inserting mail still
-- rebuilt every role for the whole thread (and every label insert rebuilt
-- them again). Both can only raise their role's newest date, so they now
-- upsert MAX(latest_date, the new message's date) instead. Starring is the
-- same raise; unstarring still recomputes only the starred role. Deleting or
-- moving mail, and removing a label, still rebuild the affected thread.
--
-- Membership, order and latest_date are unchanged; 114 does not touch rows.

DROP TRIGGER IF EXISTS thread_role_pages_message_insert;
DROP TRIGGER IF EXISTS thread_role_pages_message_update;
DROP TRIGGER IF EXISTS thread_role_pages_message_flags;
DROP TRIGGER IF EXISTS thread_role_pages_label_insert;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_insert
AFTER INSERT ON messages
BEGIN
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    VALUES ('all_mail', NEW.account_id, NEW.thread_id, CASE WHEN NEW.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE NEW.date END)
    ON CONFLICT(role, account_id, thread_id)
    DO UPDATE SET latest_date = MAX(latest_date, excluded.latest_date);

    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', NEW.account_id, NEW.thread_id, CASE WHEN NEW.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE NEW.date END
    WHERE (NEW.flags & 2) != 0
    ON CONFLICT(role, account_id, thread_id)
    DO UPDATE SET latest_date = MAX(latest_date, excluded.latest_date);

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
           END), NEW.account_id, NEW.thread_id, CASE WHEN NEW.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE NEW.date END
      FROM message_labels ml
      JOIN labels l ON l.id = ml.label_id
     WHERE ml.message_id = NEW.id
       AND l.kind = 'system'
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
    ON CONFLICT(role, account_id, thread_id)
    DO UPDATE SET latest_date = MAX(latest_date, excluded.latest_date);
END;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_update
AFTER UPDATE OF account_id, thread_id, date ON messages
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

CREATE TRIGGER IF NOT EXISTS thread_role_pages_message_flags
AFTER UPDATE OF flags ON messages
WHEN (OLD.flags & 2) != (NEW.flags & 2)
BEGIN
    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', NEW.account_id, NEW.thread_id, CASE WHEN NEW.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE NEW.date END
     WHERE (NEW.flags & 2) != 0
    ON CONFLICT(role, account_id, thread_id)
    DO UPDATE SET latest_date = MAX(latest_date, excluded.latest_date);

    DELETE FROM thread_role_pages
     WHERE account_id = NEW.account_id
       AND thread_id = NEW.thread_id
       AND (NEW.flags & 2) = 0;

    INSERT INTO thread_role_pages (role, account_id, thread_id, latest_date)
    SELECT 'starred', m.account_id, m.thread_id,
           MAX(CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END)
      FROM messages m
     WHERE m.account_id = NEW.account_id
       AND m.thread_id = NEW.thread_id
       AND (m.flags & 2) != 0
     GROUP BY m.account_id, m.thread_id
    ON CONFLICT(role, account_id, thread_id)
    DO UPDATE SET latest_date = MAX(latest_date, excluded.latest_date);
END;

CREATE TRIGGER IF NOT EXISTS thread_role_pages_label_insert
AFTER INSERT ON message_labels
BEGIN
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
           END), m.account_id, m.thread_id, CASE WHEN m.date > CAST(strftime('%s', 'now') AS INTEGER) + 86400 THEN 0 ELSE m.date END
      FROM messages m
      JOIN labels l ON l.id = NEW.label_id
     WHERE m.id = NEW.message_id
       AND l.kind = 'system'
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
    ON CONFLICT(role, account_id, thread_id)
    DO UPDATE SET latest_date = MAX(latest_date, excluded.latest_date);
END;
