import { Component, inject, signal } from '@angular/core';
import { Router, RouterLink } from '@angular/router';

@Component({
  imports: [RouterLink],
  selector: 'app-root',
  styleUrl: './landing.css',
  templateUrl: './landing.html',
})
export class Landing {
  protected readonly title = signal('landing');

  router = inject(Router);
  errorMessage = signal('');

  /** Navigates to the Login page. */
  openLogin(): void {
    this.router.navigate(['/login']);
  }

  /** Navigates to the Signup page. */
  openSignup(): void {
    this.router.navigate(['/signup']);
  }
}
