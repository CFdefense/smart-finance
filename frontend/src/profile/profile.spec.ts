import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { of, throwError } from 'rxjs';
import { UserProfile } from './profile';
import { ProfileService } from '../app/services/profile.service';
import { PlaidService, type PlaidAccount } from '../app/services/plaid.service';

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
        {
          provide: PlaidService,
          useValue: {
            getAccounts: vi.fn().mockReturnValue(of({ accounts: [] })),
          },
        },

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

/** Account-management behavior, using mocked HTTP services and a fake Plaid SDK. */
describe('UserProfile - Plaid account management', () => {
  let component: UserProfile;
  const profileServiceMock = {
    getProfile: vi.fn(),
    updateProfile: vi.fn(),
    changePassword: vi.fn(),
  };
  const plaidServiceMock = {
    getAccounts: vi.fn(),
    createLinkToken: vi.fn(),
    exchangeToken: vi.fn(),
    removeItem: vi.fn(),
  };

  const checking: PlaidAccount = {
    account_id: 'account-checking',
    item_id: 'item-123',
    name: 'Checking',
    official_name: null,
    account_type: 'depository',
    account_subtype: 'checking',
    mask: '1234',
    current_balance: 500,
    available_balance: 450,
    currency_code: 'USD',
  };
  const savings: PlaidAccount = {
    ...checking,
    account_id: 'account-savings',
    name: 'Savings',
    account_subtype: 'savings',
    mask: '5678',
  };

  beforeEach(async () => {
    vi.clearAllMocks();
    profileServiceMock.getProfile.mockReturnValue(
      of({
        first_name: 'Test',
        last_name: 'User',
        email: 'test@example.com',
        email_notifications: false,
        phone_number: null,
        sms_notifications: false,
        sms_carrier_gateway: null,
      }),
    );
    plaidServiceMock.getAccounts.mockReturnValue(of({ accounts: [] }));

    await TestBed.configureTestingModule({
      imports: [UserProfile],
      providers: [
        provideRouter([]),
        { provide: ProfileService, useValue: profileServiceMock },
        { provide: PlaidService, useValue: plaidServiceMock },
      ],
    })
      .overrideComponent(UserProfile, { set: { template: '' } })
      .compileComponents();
    component = TestBed.createComponent(UserProfile).componentInstance;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('loads connected accounts successfully', () => {
    plaidServiceMock.getAccounts.mockReturnValue(of({ accounts: [checking] }));
    component.loadAccounts();
    expect(component.accounts()).toEqual([checking]);
    expect(component.accountsLoading()).toBe(false);
    expect(component.accountsError()).toBe('');
  });

  it('shows an error if loading accounts fails', () => {
    plaidServiceMock.getAccounts.mockReturnValue(throwError(() => new Error('Network error')));
    component.loadAccounts();
    expect(component.accountsLoading()).toBe(false);
    expect(component.accountsError()).toBe('Unable to load connected accounts.');
  });

  it('groups multiple accounts from the same bank', () => {
    component.accounts.set([checking, savings]);
    const groups = component.linkedBanks();
    expect(groups).toHaveLength(1);
    expect(groups[0].itemId).toBe('item-123');
    expect(groups[0].accounts).toHaveLength(2);
  });

  it('does not disconnect when the user cancels', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    component.removeBank('item-123');
    expect(plaidServiceMock.removeItem).not.toHaveBeenCalled();
  });

  it('disconnects a bank after confirmation', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    plaidServiceMock.removeItem.mockReturnValue(of(undefined));
    plaidServiceMock.getAccounts.mockReturnValue(of({ accounts: [] }));
    component.accounts.set([checking]);
    component.removeBank('item-123');
    expect(plaidServiceMock.removeItem).toHaveBeenCalledWith('item-123');
    expect(component.accounts()).toEqual([]);
    expect(component.removingItem()).toBe(null);
    expect(component.accountsMessage()).toBe('Bank disconnected successfully.');
  });

  it('preserves accounts if disconnection fails', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    plaidServiceMock.removeItem.mockReturnValue(throwError(() => new Error('Plaid unavailable')));
    component.accounts.set([checking]);
    component.removeBank('item-123');
    expect(component.accounts()).toEqual([checking]);
    expect(component.removingItem()).toBe(null);
    expect(component.accountsError()).toBe('Unable to disconnect the bank.');
  });

  /** Capture Plaid callbacks without opening a real Plaid window. */
  function mockPlaidLink() {
    const handler = { open: vi.fn(), destroy: vi.fn() };
    const create = vi.fn(
      (config: {
        token: string;
        onSuccess: (publicToken: string | null) => void;
        onExit: (error: { error_message?: string } | null) => void;
      }) => {
        void config;
        return handler;
      },
    );
    vi.stubGlobal('Plaid', { create });
    return { handler, create };
  }

  it('shows an error if the Plaid SDK is unavailable', () => {
    vi.stubGlobal('Plaid', undefined);
    component.connectBank();
    expect(component.accountsError()).toBe('Plaid could not load. Please refresh the page.');
    expect(component.connectingBank()).toBe(false);
    expect(plaidServiceMock.createLinkToken).not.toHaveBeenCalled();
  });

  it('handles failure when requesting a Link token', () => {
    mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(
      throwError(() => new Error('Backend unavailable')),
    );
    component.connectBank();
    expect(component.accountsError()).toContain('Unable to start bank connection');
    expect(component.connectingBank()).toBe(false);
  });

  it('opens Plaid Link with the token from the backend', () => {
    const { handler, create } = mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(of({ link_token: 'link-test-123' }));
    component.connectBank();
    expect(create).toHaveBeenCalledOnce();
    expect(create.mock.calls[0][0].token).toBe('link-test-123');
    expect(handler.open).toHaveBeenCalledOnce();
    expect(component.connectingBank()).toBe(true);
    component.connectBank();
    expect(plaidServiceMock.createLinkToken).toHaveBeenCalledTimes(1);
  });

  it('handles the user canceling Plaid Link', () => {
    const { handler, create } = mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(of({ link_token: 'link-test' }));
    component.connectBank();
    create.mock.calls[0][0].onExit(null);
    expect(component.connectingBank()).toBe(false);
    expect(component.accountsError()).toBe('');
    expect(handler.destroy).toHaveBeenCalled();
    expect(plaidServiceMock.exchangeToken).not.toHaveBeenCalled();
  });

  it('displays an error when Plaid Link exits unsuccessfully', () => {
    const { create } = mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(of({ link_token: 'link-test' }));
    component.connectBank();
    create.mock.calls[0][0].onExit({
      error_message: 'Bank authentication failed',
    });
    expect(component.accountsError()).toBe('Bank authentication failed');
    expect(component.connectingBank()).toBe(false);
  });

  it('exchanges the public token after successful linking', () => {
    const { handler, create } = mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(of({ link_token: 'link-test' }));
    plaidServiceMock.exchangeToken.mockReturnValue(of({ success: true, item_id: 'item-new' }));
    plaidServiceMock.getAccounts.mockReturnValue(of({ accounts: [checking] }));
    component.connectBank();
    create.mock.calls[0][0].onSuccess('public-test-123');
    expect(plaidServiceMock.exchangeToken).toHaveBeenCalledWith('public-test-123');
    expect(plaidServiceMock.getAccounts).toHaveBeenCalled();
    expect(component.accounts()).toEqual([checking]);
    expect(component.accountsMessage()).toBe('Bank connected successfully!');
    expect(component.connectingBank()).toBe(false);
    expect(handler.destroy).toHaveBeenCalled();
  });

  it('handles a failed public token exchange', () => {
    const { handler, create } = mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(of({ link_token: 'link-test' }));
    plaidServiceMock.exchangeToken.mockReturnValue(
      throwError(() => new Error('Token exchange failed')),
    );
    component.connectBank();
    create.mock.calls[0][0].onSuccess('public-test');
    expect(component.accountsError()).toBe(
      'The bank was linked, but saving the connection failed.',
    );
    expect(component.connectingBank()).toBe(false);
    expect(handler.destroy).toHaveBeenCalled();
  });

  it('handles a missing public token from Plaid', () => {
    const { handler, create } = mockPlaidLink();
    plaidServiceMock.createLinkToken.mockReturnValue(of({ link_token: 'link-test' }));
    component.connectBank();
    create.mock.calls[0][0].onSuccess(null);
    expect(component.accountsError()).toBe('Plaid did not return a public token.');
    expect(component.connectingBank()).toBe(false);
    expect(plaidServiceMock.exchangeToken).not.toHaveBeenCalled();
    expect(handler.destroy).toHaveBeenCalled();
  });
});
