/** Shared profile API data models. */

/**
 * Represents the user's profile information and notification preferences.
 */
export interface UserProfile {
  first_name: string;
  last_name: string;
  email: string;
  email_notifications: boolean;
  phone_number: string | null;
  sms_notifications: boolean;
  sms_carrier_gateway: string | null;
}

/**
 * Defines the optional fields that can be changed in a user's profile.
 */
export interface UpdateProfileRequest {
  first_name?: string;
  last_name?: string;
  email?: string;
  email_notifications?: boolean;
  phone_number?: string;
  sms_notifications?: boolean;
  sms_carrier_gateway?: string;
}

/**
 * Contains the current and new passwords for a password change request.
 */
export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}
