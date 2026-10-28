import { inject, Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';

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

/**
 * Handles HTTP requests for profile management and password changes.
 */
@Injectable({
  providedIn: 'root',
})
export class ProfileService {
  private readonly http = inject(HttpClient);
  private readonly baseUrl = 'http://localhost:3001/api/user';

  /**
   * Retrieves the authenticated user's profile information.
   */
  getProfile(): Observable<UserProfile> {
    return this.http.get<UserProfile>(`${this.baseUrl}/profile`, { withCredentials: true });
  }

  /**
   * Updates the authenticated user's profile using the provided changes.
   */
  updateProfile(changes: UpdateProfileRequest): Observable<void> {
    return this.http.put<void>(`${this.baseUrl}/profile`, changes, { withCredentials: true });
  }

  /**
   * Changes the authenticated user's password.
   */
  changePassword(passwords: ChangePasswordRequest): Observable<void> {
    return this.http.put<void>(`${this.baseUrl}/password`, passwords, { withCredentials: true });
  }
}
