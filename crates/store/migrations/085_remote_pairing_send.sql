ALTER TABLE remote_pairings ADD COLUMN allow_send INTEGER NOT NULL DEFAULT 0 CHECK(allow_send IN (0,1));
