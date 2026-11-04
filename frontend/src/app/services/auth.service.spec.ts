import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { AuthService } from './auth.service';

describe('AuthService', () => {
  let service: AuthService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [AuthService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(AuthService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  /**
   * Checks that login sends the expected credentials and includes cookies.
   * Intercepts the POST request and verifies its URL, body, and credentials flag.
   */
  it('should send login credentials to the backend', () => {
    service.login('john@example.com', 'Password123!').subscribe();

    const req = httpMock.expectOne('http://localhost:3001/api/user/login');

    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual({
      email: 'john@example.com',
      password: 'Password123!',
    });
    expect(req.request.withCredentials).toBe(true);

    req.flush(null);
  });

  /**
   * Checks that signup sends all required registration fields.
   * Intercepts the POST request and verifies its payload and credentials flag.
   */
  it('should send signup information to the backend', () => {
    service.signup('John', 'Smith', 'john@example.com', 'Password123!').subscribe();

    const req = httpMock.expectOne('http://localhost:3001/api/user/signup');

    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual({
      first_name: 'John',
      last_name: 'Smith',
      email: 'john@example.com',
      password: 'Password123!',
    });
    expect(req.request.withCredentials).toBe(true);

    req.flush(null);
  });

  /**
   * Checks that the service requests the current user's auth status.
   * Intercepts the GET /api/user/me request and verifies credentials are included.
   */
  it('should check authentication status', () => {
    service.checkAuth().subscribe();

    const req = httpMock.expectOne('http://localhost:3001/api/user/me');

    expect(req.request.method).toBe('GET');
    expect(req.request.withCredentials).toBe(true);

    req.flush(null);
  });

  /**
   * Checks that logout sends the expected authenticated request.
   * Intercepts the POST request and verifies its empty body and credentials flag.
   */
  it('should send a logout request', () => {
    service.logout().subscribe();

    const req = httpMock.expectOne('http://localhost:3001/api/user/logout');

    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual({});
    expect(req.request.withCredentials).toBe(true);

    req.flush(null);
  });
});
