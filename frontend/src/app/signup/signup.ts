import { Component, inject, signal } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { AuthService } from '../services/auth.service';

@Component({
  imports: [RouterLink, FormsModule],
  selector: 'app-signup',
  styleUrl: './signup.css',
  templateUrl: './signup.html',
})
export class Signup {
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  firstName = '';
  lastName = '';
  email = '';
  password = '';

  errorMessage = signal('');
  isSubmitting = signal(false);

  onSignup(): void {
    if (this.isSubmitting()) {
      return;
    }

    this.errorMessage.set('');

    if (!this.firstName.trim() || !this.lastName.trim() || !this.email.trim() || !this.password) {
      this.errorMessage.set('Please complete all fields.');
      return;
    }

    this.isSubmitting.set(true);

    this.authService
      .signup(this.firstName.trim(), this.lastName.trim(), this.email.trim(), this.password)
      .subscribe({
        next: () => {
          this.isSubmitting.set(false);
          this.router.navigate(['/dashboard']);
        },
        error: (err) => {
          this.isSubmitting.set(false);

          this.errorMessage.set(
            err.status === 409
              ? 'An account with this email already exists.'
              : err.status === 400 || err.status === 422
                ? 'Please check your information and password requirements.'
                : 'Registration failed. Please try again later.',
          );
        },
      });
  }
}
