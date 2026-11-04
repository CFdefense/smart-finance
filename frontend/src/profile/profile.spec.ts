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

  /**
   * Checks that profile data loads into the component's editable fields.
   * Uses the mocked profile response and verifies fields and loading state.
   */
  it('should load profile information', () => {
    expect(profileService.getProfile).toHaveBeenCalled();
    expect(component.firstName).toBe('John');
    expect(component.lastName).toBe('Smith');
    expect(component.email).toBe('john@example.com');
    expect(component.emailNotifications).toBe(true);
    expect(component.loading()).toBe(false);
  });

  /**
   * Checks that a profile cannot be saved without a first name.
   * Clears the first name and verifies validation prevents an update request.
   */
  it('should reject an empty first name', () => {
    component.firstName = '';

    component.saveProfile();

    expect(component.errorMessage()).toBe('First and last name are required.');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  /**
   * Checks that malformed email addresses are rejected before saving.
   * Enters an invalid email and verifies the error and absent update request.
   */
  it('should reject an invalid email', () => {
    component.email = 'not-an-email';

    component.saveProfile();

    expect(component.errorMessage()).toBe('Please enter a valid email address.');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  /**
   * Checks that the profile enforces the required phone number format.
   * Supplies a short number and verifies validation blocks the update.
   */
  it('should reject an invalid phone number', () => {
    component.phoneNumber = '12345';

    component.saveProfile();

    expect(component.errorMessage()).toContain('Phone number must include the country code');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  /**
   * Checks that valid profile edits are saved and refreshed.
   * Updates the first name and verifies the payload, success message, and reload.
   */
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

  /**
   * Checks that conflicting email changes show a specific error.
   * Mocks a 409 update response and verifies the message and saving state.
   */
  it('should display an error for a duplicate email', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 409 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('That email address is already in use.');
    expect(component.saving()).toBe(false);
  });

  /**
   * Checks that password confirmation must match the new password.
   * Supplies different values and verifies the password service is not called.
   */
  it('should reject mismatched passwords', () => {
    component.newPassword = 'Password123!';
    component.confirmPassword = 'Different123!';

    component.updatePassword();

    expect(component.errorMessage()).toBe('New passwords do not match.');
    expect(profileService.changePassword).not.toHaveBeenCalled();
  });

  /**
   * Checks that successful password updates clear sensitive form values.
   * Submits matching passwords and verifies the payload, cleared fields, and message.
   */
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

  /**
   * Checks that failed profile retrieval shows an error and stops loading.
   * Mocks a 500 fetch response and verifies the error and loading state.
   */
  it('should display an error when loading the profile fails', () => {
    profileService.getProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 500 })),
    );

    component.loadProfile();

    expect(component.errorMessage()).toBe('Unable to load your profile.');
    expect(component.loading()).toBe(false);
  });

  /**
   * Checks that whitespace-only last names cannot be saved.
   * Enters spaces and verifies validation prevents an update request.
   */
  it('should reject an empty last name', () => {
    component.lastName = '   ';

    component.saveProfile();

    expect(component.errorMessage()).toBe('First and last name are required.');
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  /**
   * Checks that SMS notifications require a phone number.
   * Enables SMS without a number and verifies validation blocks saving.
   */
  it('should reject SMS notifications without a phone number', () => {
    component.smsNotifications = true;
    component.phoneNumber = '';

    component.saveProfile();

    expect(component.errorMessage()).toBe(
      'Please enter a phone number to enable SMS notifications.',
    );
    expect(profileService.updateProfile).not.toHaveBeenCalled();
  });

  /**
   * Checks that SMS notifications require a selected carrier.
   * Enables SMS with a phone but no carrier and verifies the validation error.
   */
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

  /**
   * Checks that removing a phone number can save with SMS disabled.
   * Turns off SMS after removing the phone and verifies the update payload.
   */
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

  /**
   * Checks that valid SMS preferences are included in profile updates.
   * Supplies a phone and carrier and verifies the submitted SMS fields.
   */
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

  /**
   * Checks that invalid profile updates show a validation-specific message.
   * Mocks a 400 update response and verifies the error and saving state.
   */
  it('should display a validation error for a 400 response', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 400 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('Please check your profile information and try again.');
    expect(component.saving()).toBe(false);
  });

  /**
   * Checks that expired sessions show the appropriate profile-save error.
   * Mocks a 401 update response and verifies the session-expired message.
   */
  it('should display a session error for a 401 response', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 401 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('Your session has expired. Please log in again.');
  });

  /**
   * Checks that profile-save server failures show a fallback message.
   * Mocks a 500 update response and verifies the general error.
   */
  it('should display a generic error for a 500 response', () => {
    profileService.updateProfile.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 500 })),
    );

    component.saveProfile();

    expect(component.errorMessage()).toBe('Unable to save profile changes. Please try again.');
  });

  /**
   * Checks that failed password changes show an error and reset progress.
   * Mocks a 400 password response and verifies the message and changing state.
   */
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

  /**
   * Checks that the back action returns the user to the dashboard.
   * Calls backToDashboard and verifies navigation to /dashboard.
   */
  it('should navigate back to the dashboard', () => {
    component.backToDashboard();

    expect(router.navigate).toHaveBeenCalledWith(['/dashboard']);
  });

  /**
   * Checks that optional SMS-related profile fields are restored on load.
   * Mocks a populated profile and verifies phone, SMS toggle, and carrier.
   */
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
