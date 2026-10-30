import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { ProfileService } from '../app/services/profile.service';
import { HttpErrorResponse } from '@angular/common/http';

@Component({
  imports: [FormsModule],
  selector: 'app-root',
  styleUrl: './profile.css',
  templateUrl: './profile.html',
})
export class UserProfile implements OnInit {
  private readonly profileService = inject(ProfileService);
  private readonly router = inject(Router);

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
   * Navigates the user back to the dashboard.
   */

  backToDashboard(): void {
    this.router.navigate(['/dashboard']);
  }
}
