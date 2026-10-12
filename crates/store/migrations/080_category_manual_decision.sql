ALTER TABLE message_category_memberships ADD COLUMN manual_decision INTEGER CHECK (manual_decision IN (0, 1));
UPDATE message_category_memberships SET manual_decision = assigned;
