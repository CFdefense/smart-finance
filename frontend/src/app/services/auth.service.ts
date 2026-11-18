import { inject, Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';
import { API_BASE_URL } from '../api-config';

@Injectable({
  providedIn: 'root',
})
export class AuthService {
  private readonly http = inject(HttpClient);

  login(email: string, password: string): Observable<void> {
    return this.http.post<void>(
      `${API_BASE_URL}/user/login`,
      { email, password },
      { withCredentials: true },
    );
  }

  signup(firstName: string, lastName: string, email: string, password: string): Observable<void> {
    return this.http.post<void>(
      `${API_BASE_URL}/user/signup`,
      {
        first_name: firstName,
        last_name: lastName,
        email,
        password,
      },
      { withCredentials: true },
    );
  }

  checkAuth(): Observable<void> {
    return this.http.get<void>(`${API_BASE_URL}/user/me`, { withCredentials: true });
  }

  logout(): Observable<void> {
    return this.http.post<void>(`${API_BASE_URL}/user/logout`, {}, { withCredentials: true });
  }
}
