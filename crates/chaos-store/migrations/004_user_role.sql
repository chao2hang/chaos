-- System rule: the first account created at install/setup is the admin.
-- Existing single-admin MVP DBs: promote earliest user to admin.

ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';

UPDATE users
SET role = 'admin'
WHERE id = (
  SELECT id FROM users ORDER BY created_at ASC, id ASC LIMIT 1
);
