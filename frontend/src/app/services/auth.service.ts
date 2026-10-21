import { inject, Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';

@Injectable({
  providedIn: 'root',
})
export class AuthService {
  private readonly http = inject(HttpClient);

  login(email: string, password: string): Observable<void> {
    return this.http.post<void>(
      'http://localhost:3001/api/user/login',
      { email, password },
      { withCredentials: true },
    );
  }

  signup(firstName: string, lastName: string, email: string, password: string): Observable<void> {
    return this.http.post<void>(
      'http://localhost:3001/api/user/signup',
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
    return this.http.get<void>('http://localhost:3001/api/user/me', { withCredentials: true });
  }

  logout(): Observable<void> {
    return this.http.post<void>(
      'http://localhost:3001/api/user/logout',
      {},
      { withCredentials: true },
    );
  }
}
