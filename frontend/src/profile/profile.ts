import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { CurrencyPipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { ProfileService } from '../app/services/profile.service';
import { PlaidAccount, PlaidService } from '../app/services/plaid.service';
import { HttpErrorResponse } from '@angular/common/http';

/**
 * Describe the Plaid SDK loaded through index.html.
 *
 * Plaid is provided by an external JavaScript script rather than
 * imported as an Angular package. This tells TypeScript which
 * functions and callbacks the global Plaid object provides.
 */
declare const Plaid: {
  create(config: {
    token: string;
    onSuccess: (publicToken: string | null) => void;
    onExit: (error: { error_message?: string } | null) => void;
  }): {
    open: () => void;
    destroy: () => void;
  };
};

@Component({
  imports: [FormsModule, CurrencyPipe],
  selector: 'app-root',
  styleUrl: './profile.css',
  templateUrl: './profile.html',
})
export class UserProfile implements OnInit {
  private readonly profileService = inject(ProfileService);
  private readonly router = inject(Router);

  private readonly plaidService = inject(PlaidService);

  // Store the user's connected financial accounts.
  accounts = signal<PlaidAccount[]>([]);

  // Track loading, removal, and account-specific errors.
  accountsLoading = signal(false);
  removingItem = signal<string | null>(null);
  accountsError = signal('');
  accountsMessage = signal('');
  // Prevent users from starting multiple bank connections at once.
  connectingBank = signal(false);

  /**
   * Group accounts by their Plaid Item ID.
   *
   * One bank connection can contain multiple accounts.
   * Grouping them ensures we show one Remove button per connection.
   */
  linkedBanks = computed(() => {
    const groups = new Map<string, PlaidAccount[]>();

    for (const account of this.accounts()) {
      const existing = groups.get(account.item_id) ?? [];
      existing.push(account);
      groups.set(account.item_id, existing);
    }

    return Array.from(groups, ([itemId, accounts]) => ({
      itemId,
      accounts,
    }));
  });

  // Controls which section of the redesigned Profile page is visible.
  currentView: 'accountInfo' | 'notifications' | 'password' | 'banks' = 'accountInfo';

  // Switch between sections without navigating away from the Profile page.
  setView(view: 'accountInfo' | 'notifications' | 'password' | 'banks'): void {
    this.currentView = view;
    this.errorMessage.set('');
  }

  firstName = '';
  lastName = '';
  email = '';
  emailNotifications = false;
  phoneNumber = '';
  smsNotifications = false;
  smsCarrierGateway = '';

  currentPassword = '';
  newPassword = '';
  confirmPassword = '';

  loading = signal(true);
  saving = signal(false);
  changingPassword = signal(false);

  profileMessage = signal('');
  passwordMessage = signal('');
  errorMessage = signal('');

  /**
   * Loads the user's profile when the component initializes.
   */

  ngOnInit(): void {
    this.loadProfile();
    this.loadAccounts();
  }

  /**
   * Retrieves the user's profile and populates the form fields.
   * Updates the loading and error states based on the response.
   */

  loadProfile(): void {
    this.loading.set(true);
    this.errorMessage.set('');

    this.profileService.getProfile().subscribe({
      next: (profile) => {
        this.firstName = profile.first_name;
        this.lastName = profile.last_name;
        this.email = profile.email;
        this.emailNotifications = profile.email_notifications;
        // Use empty strings for optional fields so the form inputs remain editable.
        this.phoneNumber = profile.phone_number ?? '';
        this.smsNotifications = profile.sms_notifications;
        this.smsCarrierGateway = profile.sms_carrier_gateway ?? '';
        this.loading.set(false);
      },
      error: () => {
        this.errorMessage.set('Unable to load your profile.');
        this.loading.set(false);
      },
    });
  }

  /**
   * Validates the profile fields and submits changes to the backend.
   * Displays the appropriate success or error message.
   */

  saveProfile(): void {
    this.profileMessage.set('');
    this.errorMessage.set('');

    if (!this.firstName.trim() || !this.lastName.trim()) {
      this.errorMessage.set('First and last name are required.');
      return;
    }

    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(this.email.trim())) {
      this.errorMessage.set('Please enter a valid email address.');
      return;
    }

    const phone = this.phoneNumber.trim();

    if (phone && !/^\+[1-9]\d{7,14}$/.test(phone)) {
      this.errorMessage.set('Phone number must include the country code, such as +15551234567.');
      return;
    }

    if (this.smsNotifications && !phone) {
      this.errorMessage.set('Please enter a phone number to enable SMS notifications.');
      return;
    }

    if (this.smsNotifications && !this.smsCarrierGateway) {
      this.errorMessage.set('Please select your mobile carrier to enable SMS notifications.');
      return;
    }

    this.saving.set(true);

    this.profileService
      .updateProfile({
        first_name: this.firstName,
        last_name: this.lastName,
        email: this.email,
        email_notifications: this.emailNotifications,
        phone_number: this.phoneNumber,
        // Disable SMS notifications when no phone number is provided.
        sms_notifications: this.phoneNumber.trim() ? this.smsNotifications : false,
        sms_carrier_gateway: this.smsCarrierGateway,
      })
      .subscribe({
        next: () => {
          this.saving.set(false);
          this.profileMessage.set('Profile updated successfully!');
          this.loadProfile();
        },

        error: (error: HttpErrorResponse) => {
          this.saving.set(false);

          if (error.status === 409) {
            this.errorMessage.set('That email address is already in use.');
          } else if (error.status === 400) {
            this.errorMessage.set('Please check your profile information and try again.');
          } else if (error.status === 401) {
            this.errorMessage.set('Your session has expired. Please log in again.');
          } else {
            this.errorMessage.set('Unable to save profile changes. Please try again.');
          }
        },
      });
  }

  /**
   * Validates the password confirmation and submits a password change.
   * Clears the password fields after a successful update.
   */

  updatePassword(): void {
    this.passwordMessage.set('');
    this.errorMessage.set('');

    if (this.newPassword !== this.confirmPassword) {
      this.errorMessage.set('New passwords do not match.');
      return;
    }

    this.changingPassword.set(true);

    this.profileService
      .changePassword({
        current_password: this.currentPassword,
        new_password: this.newPassword,
      })
      .subscribe({
        next: () => {
          this.changingPassword.set(false);
          this.currentPassword = '';
          this.newPassword = '';
          this.confirmPassword = '';
          this.passwordMessage.set('Password updated successfully!');
        },
        error: () => {
          this.changingPassword.set(false);
          this.errorMessage.set('Unable to change password. Check your current and new passwords.');
        },
      });
  }

  /**
   * Retrieve all connected financial accounts.
   *
   * This follows the same pattern as loadProfile():
   * set loading state, call a service, and process the response.
   */
  loadAccounts(): void {
    this.accountsLoading.set(true);
    this.accountsError.set('');

    this.plaidService.getAccounts().subscribe({
      next: (response) => {
        // Update the signal so Angular refreshes the account list.
        this.accounts.set(response.accounts);
        this.accountsLoading.set(false);
      },
      error: () => {
        this.accountsError.set('Unable to load connected accounts.');
        this.accountsLoading.set(false);
      },
    });
  }

  /**
   * Open Plaid Link so the user can connect a financial institution.
   *
   * Flow:
   * 1. Request a Link token from Rust.
   * 2. Open Plaid's connection interface.
   * 3. Receive a public token after successful linking.
   * 4. Send the public token to Rust for secure exchange.
   * 5. Refresh the user's displayed accounts.
   */
  connectBank(): void {
    // Don't start another connection while one is in progress.
    if (this.connectingBank()) {
      return;
    }

    this.connectingBank.set(true);
    this.accountsError.set('');
    this.accountsMessage.set('');

    // The SDK must have loaded from the script in index.html.
    if (typeof Plaid === 'undefined') {
      this.accountsError.set('Plaid could not load. Please refresh the page.');
      this.connectingBank.set(false);
      return;
    }

    // STEP 1: Request a temporary Link token from our Rust backend.
    this.plaidService.createLinkToken().subscribe({
      next: (response) => {
        // STEP 2: Configure the Plaid Link interface.
        const handler = Plaid.create({
          token: response.link_token,

          // STEP 3: Plaid calls this after the user connects a bank.
          onSuccess: (publicToken) => {
            if (!publicToken) {
              this.accountsError.set('Plaid did not return a public token.');
              this.connectingBank.set(false);
              handler.destroy();
              return;
            }

            // STEP 4: Give Rust the temporary public token.
            // Rust exchanges it for an access token stored in PostgreSQL.
            this.plaidService.exchangeToken(publicToken).subscribe({
              next: () => {
                this.connectingBank.set(false);
                this.accountsMessage.set('Bank connected successfully!');

                // STEP 5: Refresh the displayed account list.
                this.loadAccounts();
                handler.destroy();
              },
              error: () => {
                this.connectingBank.set(false);
                this.accountsError.set('The bank was linked, but saving the connection failed.');
                handler.destroy();
              },
            });
          },

          // Called when the user exits without completing the connection.
          onExit: (error) => {
            this.connectingBank.set(false);

            if (error) {
              this.accountsError.set(error.error_message || 'Unable to connect the bank.');
            }

            // Clean up the Plaid interface.
            handler.destroy();
          },
        });

        // Display Plaid's interface to the user.
        handler.open();
      },

      // The backend couldn't create a Link token.
      error: () => {
        this.connectingBank.set(false);
        this.accountsError.set('Unable to start bank connection. Please try again.');
      },
    });
  }

  /**
   * Disconnect a bank connection after asking for confirmation.
   *
   * Removing a Plaid Item can disconnect multiple accounts and
   * delete associated local transaction history.
   */
  removeBank(itemId: string): void {
    const confirmed = window.confirm(
      'Disconnect this bank? All accounts and locally stored transactions ' +
        'associated with this connection will be removed.',
    );

    // Do nothing if the user cancels.
    if (!confirmed) {
      return;
    }

    this.removingItem.set(itemId);
    this.accountsError.set('');
    this.accountsMessage.set('');

    // Send the DELETE request through our Plaid Angular service.
    this.plaidService.removeItem(itemId).subscribe({
      next: () => {
        this.removingItem.set(null);
        this.accountsMessage.set('Bank disconnected successfully.');

        // Reload the list to reflect the updated backend state.
        this.loadAccounts();
      },
      error: () => {
        this.removingItem.set(null);
        this.accountsError.set('Unable to disconnect the bank.');
      },
    });
  }

  /**
   * Navigates the user back to the dashboard.
   */
  backToDashboard(): void {
    this.router.navigate(['/dashboard']);
  }
}
