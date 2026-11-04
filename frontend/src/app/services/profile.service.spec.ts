import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { ProfileService, UserProfile } from './profile.service';
import { describe, it, expect, beforeEach, afterEach } from 'vitest';

describe('ProfileService', () => {
  let service: ProfileService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [ProfileService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(ProfileService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  /**
   * Checks that profile loading requests and returns the user's data.
   * Mocks the GET response and verifies the URL, cookie flag, and returned profile.
   */
  it('should retrieve the authenticated user profile', () => {
    const mockProfile: UserProfile = {
      first_name: 'John',
      last_name: 'Smith',
      email: 'john@example.com',
      email_notifications: true,
      phone_number: null,
      sms_notifications: false,
      sms_carrier_gateway: null,
    };

    service.getProfile().subscribe((profile) => {
      expect(profile).toEqual(mockProfile);
    });

    const req = httpMock.expectOne('http://localhost:3001/api/user/profile');

    expect(req.request.method).toBe('GET');
    expect(req.request.withCredentials).toBe(true);

    req.flush(mockProfile);
  });

  /**
   * Checks that profile edits are submitted with the supplied fields.
   * Intercepts the PUT request and verifies its URL, body, and credentials flag.
   */
  it('should send profile updates to the backend', () => {
    const changes = {
      first_name: 'Jane',
      email: 'jane@example.com',
      email_notifications: false,
    };

    service.updateProfile(changes).subscribe();

    const req = httpMock.expectOne('http://localhost:3001/api/user/profile');

    expect(req.request.method).toBe('PUT');
    expect(req.request.body).toEqual(changes);
    expect(req.request.withCredentials).toBe(true);

    req.flush(null);
  });

  /**
   * Checks that password changes send both current and new passwords.
   * Intercepts the PUT request and verifies the password payload and credentials.
   */
  it('should send password changes to the backend', () => {
    const passwords = {
      current_password: 'OldPassword123!',
      new_password: 'NewPassword123!',
    };

    service.changePassword(passwords).subscribe();

    const req = httpMock.expectOne('http://localhost:3001/api/user/password');

    expect(req.request.method).toBe('PUT');
    expect(req.request.body).toEqual(passwords);
    expect(req.request.withCredentials).toBe(true);

    req.flush(null);
  });
});
