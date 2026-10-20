import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { inject } from '@angular/core';
import { Router } from '@angular/router';
import { AuthService } from '../services/auth.services';
import is from '@angular/common/locales/extra/is';

@Component({
  imports: [RouterLink, FormsModule],
  selector: 'app-signup',
  styleUrl: './signup.css',
  templateUrl: './signup.html',
})
export class Signup {
  firstName = '';
  lastName = '';
  email = '';
  password = '';

  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  errorMessage = '';
  isSubmitting = false;

  onSignup() {
    this.errorMessage = '';
    this.isSubmitting = true;

    this.authService
      .signup(this.firstName, this.lastName, this.email, this.password)
      .subscribe({
        next: () => {
          this.isSubmitting = false;
          this.router.navigate(['/dashboard']);
        },
        error: (err) => {
          this.isSubmitting = false;
          this.errorMessage =
            err.status === 409
              ? 'An account with this email already exists.'
              : 'Registration failed. Please check your information.';
        },
      });
  }
}
