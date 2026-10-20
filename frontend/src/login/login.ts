import { Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { AuthService } from '../app/services/auth.services';

@Component({
  imports: [FormsModule],
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

  onLogin(): void {
    this.authService.login(this.email, this.password).subscribe({
      next: () => {
        console.log('Login successful');
        this.router.navigate(['/dashboard']);
      },
      error: (err) => {
        console.error('Login failed:', err.status);
        
      },
    });
  }
}