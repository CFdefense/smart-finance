-- migrations/007_add_notification_preferences.sql
ALTER TABLE users
  ADD COLUMN email_notifications BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN phone_number        TEXT,
  ADD COLUMN sms_notifications   BOOLEAN NOT NULL DEFAULT false,
  ADD CONSTRAINT phone_number_e164
    CHECK (phone_number IS NULL OR phone_number ~ '^\+[1-9]\d{7,14}$'),
  ADD CONSTRAINT sms_requires_phone
    CHECK (sms_notifications = false OR phone_number IS NOT NULL);
