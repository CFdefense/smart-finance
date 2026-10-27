
import { inject, Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';

export interface UserProfile {
  first_name: string;
  last_name: string;
  email: string;
  email_notifications: boolean;
  phone_number: string | null;
  sms_notifications: boolean;
}

export interface UpdateProfileRequest {
  first_name?: string;
  last_name?: string;
  email?: string;
  email_notifications?: boolean;
  phone_number?: string;
  sms_notifications?: boolean;
}

export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

@Injectable({
  providedIn: 'root',
})
export class ProfileService {
  private readonly http = inject(HttpClient);
  private readonly baseUrl = 'http://localhost:3001/api/user';

  getProfile(): Observable<UserProfile> {
    return this.http.get<UserProfile>(
      `${this.baseUrl}/profile`,
      { withCredentials: true },
    );
  }

  updateProfile(
    changes: UpdateProfileRequest,
  ): Observable<void> {
    return this.http.put<void>(
      `${this.baseUrl}/profile`,
      changes,
      { withCredentials: true },
    );
  }

  changePassword(
    passwords: ChangePasswordRequest,
  ): Observable<void> {
    return this.http.put<void>(
      `${this.baseUrl}/password`,
      passwords,
      { withCredentials: true },
    );
  }
}
