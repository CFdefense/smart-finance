import { Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

@Component({
  imports: [],
  selector: 'app-root',
  styleUrl: './profile.css',
  templateUrl: './profile.html',
})
export class UserProfile {
  protected readonly title = signal('profile');
  router = inject(Router);

  backToDashboard(): void {
    this.router.navigate(['/dashboard']);
  }
}
