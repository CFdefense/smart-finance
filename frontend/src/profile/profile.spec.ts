import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { of, throwError } from 'rxjs';

import { UserProfile } from './profile';
import { ProfileService } from '../app/services/profile.service';

describe('UserProfile', () => {
  let component: UserProfile;
  let profileService: {
    getProfile: ReturnType<typeof vi.fn>;
    updateProfile: ReturnType<typeof vi.fn>;
    changePassword: ReturnType<typeof vi.fn>;
  };

  let router: { navigate: ReturnType<typeof vi.fn> };

  const mockProfile = {
    first_name: 'John',
    last_name: 'Smith',
    email: 'john@example.com',
    email_notifications: true,
    phone_number: null,
    sms_notifications: false,
    sms_carrier_gateway: null,
  };

  beforeEach(async () => {
    profileService = {
      getProfile: vi.fn().mockReturnValue(of(mockProfile)),
      updateProfile: vi.fn().mockReturnValue(of(void 0)),
      changePassword: vi.fn().mockReturnValue(of(void 0)),
    };

    router = { navigate: vi.fn() };

    await TestBed.configureTestingModule({
      imports: [UserProfile],
      providers: [
        { provide: ProfileService, useValue: profileService },
        { provide: Router, useValue: router },
      ],
    })
      .overrideComponent(UserProfile, {
        set: { template: '' },
      })
      .compileComponents();

    const fixture = TestBed.createComponent(UserProfile);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should load profile information', () => {
    expect(profileService.getProfile).toHaveBeenCalled();
    expect(component.firstName).toBe('John');
    expect(component.lastName).toBe('Smith');
    expect(component.email).toBe('john@example.com');
    expect(component.emailNotifications).toBe(true);
    expect(component.loading()).toBe(false);
  });

  it('should reject an empty first name', () => {
    component.firstName = '';

    component.saveProfile();

    expect(component.errorMessage()).toBe('First and last name are required.');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  it('should reject an invalid email', () => {
    component.email = 'not-an-email';

    component.saveProfile();

    expect(component.errorMessage()).toBe('Please enter a valid email address.');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  it('should reject an invalid phone number', () => {
    component.phoneNumber = '12345';

    component.saveProfile();

    expect(component.errorMessage()).toContain('Phone number must include the country code');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  it('should save valid profile information', () => {
    component.firstName = 'Jane';

    component.saveProfile();

    expect(profileService.updateProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        first_name: 'Jane',
        email: 'john@example.com',
      }),
    );

    expect(component.profileMessage()).toBe('Profile updated successfully!');
    expect(component.saving()).toBe(false);
    expect(profileService.getProfile).toHaveBeenCalledTimes(2);
  });

  it('should display an error for a duplicate email', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 409 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('That email address is already in use.');
    expect(component.saving()).toBe(false);
  });

  it('should reject mismatched passwords', () => {
    component.newPassword = 'Password123!';
    component.confirmPassword = 'Different123!';

    component.updatePassword();

    expect(component.errorMessage()).toBe('New passwords do not match.');
    expect(profileService.changePassword).not.toHaveBeenCalled();
  });

  it('should clear password fields after a successful change', () => {
    component.currentPassword = 'OldPassword123!';
    component.newPassword = 'NewPassword123!';
    component.confirmPassword = 'NewPassword123!';

    component.updatePassword();

    expect(profileService.changePassword).toHaveBeenCalledWith({
      current_password: 'OldPassword123!',
      new_password: 'NewPassword123!',
    });

    expect(component.currentPassword).toBe('');
    expect(component.newPassword).toBe('');
    expect(component.confirmPassword).toBe('');
    expect(component.passwordMessage()).toBe('Password updated successfully!');
  });

  it('should display an error when loading the profile fails', () => {
    profileService.getProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 500 })),
    );

    component.loadProfile();

    expect(component.errorMessage()).toBe('Unable to load your profile.');
    expect(component.loading()).toBe(false);
  });

  it('should reject an empty last name', () => {
    component.lastName = '   ';

    component.saveProfile();

    expect(component.errorMessage()).toBe('First and last name are required.');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  it('should reject SMS notifications without a phone number', () => {
    component.smsNotifications = true;
    component.phoneNumber = '';

    component.saveProfile();

    expect(component.errorMessage()).toBe(
      'Please enter a phone number to enable SMS notifications.',
    );
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  it('should reject SMS notifications without a carrier', () => {
    component.smsNotifications = true;
    component.phoneNumber = '+15551234567';
    component.smsCarrierGateway = '';

    component.saveProfile();

    expect(component.errorMessage()).toBe(
      'Please select your mobile carrier to enable SMS notifications.',
    );
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  it('should disable SMS notifications when the phone number is removed', () => {
    component.smsNotifications = true;
    component.phoneNumber = '';

    // SMS must be disabled before saving a profile without a phone.
    component.smsNotifications = false;
    component.saveProfile();

    expect(profileService.updateProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        phone_number: '',
        sms_notifications: false,
      }),
    );
  });

  it('should save SMS notification settings with a valid phone and carrier', () => {
    component.phoneNumber = '+15551234567';
    component.smsNotifications = true;
    component.smsCarrierGateway = 'vtext.com';

    component.saveProfile();

    expect(profileService.updateProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        phone_number: '+15551234567',
        sms_notifications: true,
        sms_carrier_gateway: 'vtext.com',
      }),
    );
  });

  it('should display a validation error for a 400 response', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 400 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('Please check your profile information and try again.');
    expect(component.saving()).toBe(false);
  });

  it('should display a session error for a 401 response', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 401 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('Your session has expired. Please log in again.');
  });

  it('should display a generic error for a 500 response', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 500 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('Unable to save profile changes. Please try again.');
  });

  it('should display an error when changing the password fails', () => {
    profileService.changePassword.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 400 })),
    );

    component.currentPassword = 'WrongPassword123!';
    component.newPassword = 'NewPassword123!';
    component.confirmPassword = 'NewPassword123!';

    component.updatePassword();

    expect(component.errorMessage()).toBe(
      'Unable to change password. Check your current and new passwords.',
    );
    expect(component.changingPassword()).toBe(false);
  });

  it('should navigate back to the dashboard', () => {
    component.backToDashboard();

    expect(router.navigate).toHaveBeenCalledWith(['/dashboard']);
  });

  it('should populate optional phone and carrier fields when loading', () => {
    profileService.getProfile.mockReturnValue(
      of({
        ...mockProfile,
        phone_number: '+15551234567',
        sms_notifications: true,
        sms_carrier_gateway: 'vtext.com',
      }),
    );

    component.loadProfile();

    expect(component.phoneNumber).toBe('+15551234567');
    expect(component.smsNotifications).toBe(true);
    expect(component.smsCarrierGateway).toBe('vtext.com');
  });
});
