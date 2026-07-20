-- Add country_code to nodes for GeoIP flag display.
ALTER TABLE nodes ADD COLUMN country_code TEXT;
