
import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { ProfileService } from '../app/services/profile.service';

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

  currentPassword = '';
  newPassword = '';
  confirmPassword = '';

  loading = signal(true);
  saving = signal(false);
  changingPassword = signal(false);

  profileMessage = signal('');
  passwordMessage = signal('');
  errorMessage = signal('');

  ngOnInit(): void {
    this.loadProfile();
  }

  loadProfile(): void {
    this.loading.set(true);
    this.errorMessage.set('');

    this.profileService.getProfile().subscribe({
      next: (profile) => {
        this.firstName = profile.first_name;
        this.lastName = profile.last_name;
        this.email = profile.email;
        this.emailNotifications = profile.email_notifications;
        this.phoneNumber = profile.phone_number ?? '';
        this.smsNotifications = profile.sms_notifications;
        this.loading.set(false);
      },
      error: () => {
        this.errorMessage.set('Unable to load your profile.');
        this.loading.set(false);
      },
    });
  }

  saveProfile(): void {
    this.profileMessage.set('');
    this.errorMessage.set('');

    if (!this.firstName.trim() || !this.lastName.trim()) {
      this.errorMessage.set('First and last name are required.');
      return;
    }

    this.saving.set(true);

    this.profileService.updateProfile({
      first_name: this.firstName,
      last_name: this.lastName,
      email: this.email,
      email_notifications: this.emailNotifications,
      phone_number: this.phoneNumber,
      sms_notifications: this.phoneNumber.trim()
        ? this.smsNotifications
        : false,
    }).subscribe({
      next: () => {
        this.saving.set(false);
        this.profileMessage.set('Profile updated successfully!');
        this.loadProfile();
      },
      error: () => {
        this.saving.set(false);
        this.errorMessage.set('Unable to save profile changes.');
      },
    });
  }

  updatePassword(): void {
    this.passwordMessage.set('');
    this.errorMessage.set('');

    if (this.newPassword !== this.confirmPassword) {
      this.errorMessage.set('New passwords do not match.');
      return;
    }

    this.changingPassword.set(true);

    this.profileService.changePassword({
      current_password: this.currentPassword,
      new_password: this.newPassword,
    }).subscribe({
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

  backToDashboard(): void {
    this.router.navigate(['/dashboard']);
  }
}
