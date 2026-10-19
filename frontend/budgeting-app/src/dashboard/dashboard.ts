import { Component, signal, inject } from '@angular/core';
import { Router } from '@angular/router';

@Component({
  imports: [],
  selector: 'app-root',
  styleUrl: './dashboard.css',
  templateUrl: './dashboard.html',
})

export class UserDashboard {
  protected readonly title = signal('dashboard');
  router = inject(Router);

  openProfile(): void {
    this.router.navigate(['/profile']);
  }

  onLogout(): void {
    this.router.navigate(['/login']);
  }
}
