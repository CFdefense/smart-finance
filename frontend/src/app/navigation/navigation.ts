import { Component, inject } from '@angular/core';
import { RouterLink, RouterLinkActive, Router } from '@angular/router';
import { AuthService } from '../services/auth.service';

/**
 * Shared navigation bar for authenticated application pages.
 *
 * The same component will be reused across Dashboard,
 * Budgets, Goals, and Rules so navigation stays consistent.
 */
@Component({
  selector: 'app-navigation',
  standalone: true,

  // RouterLink allows navigation without full page reloads.
  // RouterLinkActive marks the link for the current route.
  imports: [RouterLink, RouterLinkActive],

  templateUrl: './navigation.html',
  styleUrl: './navigation.css',
})
export class Navigation {
  private readonly router = inject(Router);
  private readonly authService = inject(AuthService);

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
