import { Component, signal, inject } from '@angular/core';
import { Router } from '@angular/router';
import { AuthService } from '../app/services/auth.service';

@Component({
  imports: [],
  selector: 'app-root',
  styleUrl: './dashboard.css',
  templateUrl: './dashboard.html',
})

export class UserDashboard {
  protected readonly title = signal('dashboard');
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  openProfile(): void {
    this.router.navigate(['/profile']);
  }

  onLogout() {
    this.authService.logout().subscribe({
      next: () => {
        this.router.navigate(['/login']);
      },
      error: (err) => {
        console.error('Logout failed:', err);
      },
    });
  }

}