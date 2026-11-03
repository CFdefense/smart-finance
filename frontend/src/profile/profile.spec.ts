import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { of, throwError } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { UserProfile } from './profile';
import { ProfileService } from '../app/services/profile.service';
import { PlaidService, PlaidAccount } from '../app/services/plaid.service';

describe('UserProfile - Plaid account management', () => {
  let component: UserProfile;

  // Mock our existing profile service so no real HTTP calls are made.
  const profileServiceMock = {
    getProfile: vi.fn(),
    updateProfile: vi.fn(),
    changePassword: vi.fn(),
  };

  // Mock the new Plaid service so our tests can control its responses.
  const plaidServiceMock = {
    getAccounts: vi.fn(),
    createLinkToken: vi.fn(),
    exchangeToken: vi.fn(),
    removeItem: vi.fn(),
  };

  // Two accounts belonging to the same connected bank.
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
    // Reset any call counts or responses from previous tests.
    vi.clearAllMocks();

    // Provide safe default responses.
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

    // Replace the HTTP services with our mock objects.
    await TestBed.configureTestingModule({
      imports: [UserProfile],
      providers: [
        provideRouter([]),
        { provide: ProfileService, useValue: profileServiceMock },
        { provide: PlaidService, useValue: plaidServiceMock },
      ],
    })
      // These tests focus on component logic rather than HTML rendering.
      .overrideComponent(UserProfile, {
        set: { template: '' },
      })
      .compileComponents();

    component = TestBed.createComponent(UserProfile).componentInstance;
  });

  afterEach(() => {
    // Restore mocked browser functions such as window.confirm.
    vi.restoreAllMocks();
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

    // One connected bank containing two financial accounts.
    expect(groups.length).toBe(1);
    expect(groups[0].itemId).toBe('item-123');
    expect(groups[0].accounts.length).toBe(2);
  });

  it('does not disconnect when the user cancels', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);

    component.removeBank('item-123');

    expect(plaidServiceMock.removeItem).not.toHaveBeenCalled();
  });

  it('disconnects a bank after confirmation', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    // Simulate successful removal from Rust.
    plaidServiceMock.removeItem.mockReturnValue(of(undefined));

    // Simulate the refreshed account list being empty.
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
});
