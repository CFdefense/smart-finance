import { Component, inject, signal } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { AuthService } from '../app/services/auth.service';

@Component({
  imports: [FormsModule, RouterLink],
  selector: 'app-root',
  styleUrl: './login.css',
  templateUrl: './login.html',
})
export class Login {
  protected readonly title = signal('login');

  router = inject(Router);
  private readonly authService = inject(AuthService);

  email = '';
  password = '';

  errorMessage = signal('');
  isSubmitting = signal(false);

  onLogin(): void {
    if (this.isSubmitting()) {
      return;
    }

    this.errorMessage.set('');

    if (!this.email.trim() || !this.password) {
      this.errorMessage.set('Please enter your email and password.');
      return;
    }

    this.isSubmitting.set(true);

    this.authService.login(this.email.trim(), this.password).subscribe({
      next: () => {
        this.isSubmitting.set(false);
        this.router.navigate(['/dashboard']);
      },
      error: (err) => {
        this.isSubmitting.set(false);

        this.errorMessage.set(
          err.status === 400 || err.status === 401
            ? 'Invalid email or password.'
            : 'Unable to log in right now. Please try again.',
        );
      },
    });
  }
}
