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

  /** Navigate to the Goals page. */
  openGoals(): void {
    this.router.navigate(['/goals']);
  }

  /**
   * Navigates to the dedicated Budgets management page.
   *
   * Both Create Budget and Edit Budget use the same page,
   * which already provides forms for both operations.
   */
  openBudgets(): void {
    this.router.navigate(['/budgets']);
  }

  /** Navigates to the User Profile page. */
  openProfile(): void {
    this.router.navigate(['/profile']);
  }

  /** Logs the user out and navigates to the Login page. */
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
