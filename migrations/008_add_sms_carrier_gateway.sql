-- migrations/008_add_sms_carrier_gateway.sql
-- Stores the email-to-SMS gateway domain for the user's carrier (e.g. tmomail.net).
-- Required when sms_notifications is true; validated alongside sms_requires_phone.
ALTER TABLE users
  ADD COLUMN sms_carrier_gateway TEXT,
  ADD CONSTRAINT sms_requires_gateway
    CHECK (sms_notifications = false OR sms_carrier_gateway IS NOT NULL);
