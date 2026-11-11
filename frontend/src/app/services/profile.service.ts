import { inject, Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';

import type {
  UserProfile,
  UpdateProfileRequest,
  ChangePasswordRequest,
} from '../models/profile.models';

export type {
  UserProfile,
  UpdateProfileRequest,
  ChangePasswordRequest,
} from '../models/profile.models';

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
