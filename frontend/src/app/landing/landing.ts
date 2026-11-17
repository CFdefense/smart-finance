import { Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

@Component({
  imports: [],
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
